pub mod config;
mod store;

use config::Config;
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use store::Store;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug)]
pub struct Failure {
    pub status: u16,
    pub code: String,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for Failure {}

pub struct State {
    config: Config,
    store: AsyncMutex<Store>,
    client: reqwest::Client,
    provider_keys: std::collections::BTreeMap<String, String>,
}

fn failure(status: StatusCode, code: &str) -> Failure {
    Failure {
        status: status.as_u16(),
        code: code.into(),
    }
}

impl State {
    pub fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        config.validate()?;
        if let Some(parent) = std::path::Path::new(&config.database)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            let existed = parent.exists();
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Restrict newly created trace directories without changing existing ones.
                if !existed {
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
                }
            }
        }
        let provider_keys = config
            .models
            .values()
            .flatten()
            .filter_map(|candidate| {
                std::env::var(&candidate.api_key_env)
                    .ok()
                    .filter(|key| !key.is_empty())
                    .map(|key| (candidate.api_key_env.clone(), key))
            })
            .collect();
        let store = Store::open(&config.database)?;
        store.expire(config.policy.retention_days)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if config.database != ":memory:" {
                std::fs::set_permissions(&config.database, std::fs::Permissions::from_mode(0o600))?;
            }
        }
        Ok(Arc::new(Self {
            config,
            store: AsyncMutex::new(store),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            provider_keys,
        }))
    }

    fn prepare(&self, mut body: Value) -> Result<Value, Failure> {
        if !body.is_object() || !body["messages"].as_array().is_some_and(|a| !a.is_empty()) {
            return Err(failure(StatusCode::BAD_REQUEST, "messages_required"));
        }
        if body["stream"].as_bool() == Some(true) {
            return Err(failure(StatusCode::BAD_REQUEST, "streaming_not_supported"));
        }
        let text = body.to_string();
        if text.len() > self.config.policy.max_body_bytes {
            return Err(failure(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large"));
        }
        // Keep token controls explicit and consistent across all providers.
        if ["max_tokens", "max_completion_tokens"]
            .iter()
            .any(|key| body.get(*key).is_some_and(|value| value.as_u64().is_none()))
        {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "invalid_output_token_limit",
            ));
        }
        let tokens = body
            .get("max_completion_tokens")
            .or_else(|| body.get("max_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(self.config.policy.max_output_tokens);
        if tokens == 0 || tokens > self.config.policy.max_output_tokens {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "output_token_limit_exceeded",
            ));
        }
        if body.get("max_completion_tokens").is_none() {
            body["max_tokens"] = json!(tokens);
        }
        Ok(body)
    }

    async fn execute(&self, trace_id: &str, body: Value) -> Result<Value, Failure> {
        let requested = body["model"]
            .as_str()
            .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "model_required"))?
            .to_owned();
        let candidates = self
            .config
            .models
            .get(&requested)
            .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "unknown_model"))?;
        let prepared = match self.prepare(body) {
            Ok(value) => value,
            Err(error) => {
                self.store
                    .lock()
                    .await
                    .record(
                        trace_id,
                        &requested,
                        &json!({"redacted":true}),
                        &json!({"blocked":true,"reason":error.code}),
                        (0.0, 0, false),
                    )
                    .map_err(storage_error)?;
                return Err(error);
            }
        };
        for candidate in candidates {
            let mut payload = prepared.clone();
            payload["model"] = json!(candidate.model);
            let provider_key = self
                .provider_keys
                .get(&candidate.api_key_env)
                .ok_or_else(|| {
                    failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "provider_credentials_unavailable",
                    )
                })?;
            let month = chrono::Utc::now().format("%Y-%m").to_string();
            let output_limit = payload
                .get("max_completion_tokens")
                .or_else(|| payload.get("max_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(self.config.policy.max_output_tokens);
            // Conservative reservation, settled against provider-reported token usage.
            let reserved = ((payload.to_string().len() + 4096) as f64
                * candidate.input_per_million
                + output_limit as f64 * candidate.output_per_million)
                / 1_000_000.0;
            if !self
                .store
                .lock()
                .await
                .reserve(&month, reserved, self.config.policy.monthly_budget_usd)
                .map_err(storage_error)?
            {
                return Err(failure(
                    StatusCode::TOO_MANY_REQUESTS,
                    "monthly_budget_exceeded",
                ));
            }
            let started = Instant::now();
            let response = self
                .client
                .post(format!(
                    "{}/chat/completions",
                    candidate.base_url.trim_end_matches('/')
                ))
                .bearer_auth(provider_key)
                .json(&payload)
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    self.store.lock().await.record(trace_id,&candidate.model,&prepared,
                        &json!({"reason":"provider_transport_failure","provider":candidate.base_url}),(0.0, started.elapsed().as_millis(), false)).map_err(storage_error)?;
                    // Ambiguous transport failures may have incurred provider cost:
                    // keep the conservative reservation rather than refunding it.
                    continue;
                }
            };
            if !response.status().is_success() {
                self.store.lock().await.record(trace_id,&candidate.model,&prepared,
                    &json!({"reason":"provider_rejected_request","status":response.status().as_u16(),"provider":candidate.base_url}),(0.0, started.elapsed().as_millis(), false)).map_err(storage_error)?;
                let retry =
                    response.status().is_server_error() || response.status().as_u16() == 429;
                self.store
                    .lock()
                    .await
                    .settle(&month, reserved, 0.0)
                    .map_err(storage_error)?;
                if retry {
                    continue;
                }
                return Err(failure(
                    StatusCode::BAD_GATEWAY,
                    "provider_rejected_request",
                ));
            }
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| failure(StatusCode::BAD_GATEWAY, "invalid_provider_response"))?
            {
                if bytes.len() + chunk.len() > 8_388_608 {
                    return Err(failure(
                        StatusCode::BAD_GATEWAY,
                        "provider_response_too_large",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            let output: Value = serde_json::from_slice(&bytes)
                .map_err(|_| failure(StatusCode::BAD_GATEWAY, "invalid_provider_response"))?;
            if !output["choices"].as_array().is_some_and(|a| !a.is_empty()) {
                return Err(failure(
                    StatusCode::BAD_GATEWAY,
                    "invalid_provider_response",
                ));
            }
            let cost = match (
                output["usage"]["prompt_tokens"].as_u64(),
                output["usage"]["completion_tokens"].as_u64(),
            ) {
                (Some(input), Some(output)) => {
                    (input as f64 * candidate.input_per_million
                        + output as f64 * candidate.output_per_million)
                        / 1_000_000.0
                }
                _ => reserved,
            };
            self.store
                .lock()
                .await
                .settle(&month, reserved, cost)
                .map_err(storage_error)?;
            self.store
                .lock()
                .await
                .record(
                    trace_id,
                    &candidate.model,
                    &prepared,
                    &output,
                    (cost, started.elapsed().as_millis(), true),
                )
                .map_err(storage_error)?;
            return Ok(output);
        }
        Err(failure(
            StatusCode::BAD_GATEWAY,
            "all_providers_unavailable",
        ))
    }
}

fn storage_error(_: anyhow::Error) -> Failure {
    failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "local_storage_unavailable",
    )
}

impl State {
    pub async fn route(&self, body: Value) -> Result<Value, Failure> {
        let id = self
            .store
            .lock()
            .await
            .create(json!({"source":"olyx-cli"}))
            .map_err(storage_error)?;
        let output = self.execute(&id, body).await;
        match &output {
            Ok(_) => {
                self.store
                    .lock()
                    .await
                    .complete(&id)
                    .map_err(storage_error)?;
            }
            Err(error) => self
                .store
                .lock()
                .await
                .fail(&id, &error.code)
                .map_err(storage_error)?,
        }
        match output {
            Ok(output) => Ok(json!({"trace_id":id,"output":output})),
            Err(error) => Err(Failure {
                status: error.status,
                code: format!("{} (trace: {})", error.code, id),
            }),
        }
    }

    pub async fn debug(&self, id: Option<&str>) -> Result<Value, Failure> {
        let store = self.store.lock().await;
        store
            .expire(self.config.policy.retention_days)
            .map_err(storage_error)?;
        match id {
            Some(id) => store
                .trace(id)
                .map_err(storage_error)?
                .ok_or_else(|| failure(StatusCode::NOT_FOUND, "trace_not_found")),
            None => Ok(json!({"data":store.list().map_err(storage_error)?})),
        }
    }

    pub async fn replay(
        &self,
        source_id: &str,
        force_model: Option<String>,
        compare_models: Vec<String>,
    ) -> Result<Value, Failure> {
        let source = self.debug(Some(source_id)).await?;
        let steps: Vec<_> = source["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["allowed"] == true)
            .cloned()
            .collect();
        if steps.is_empty() {
            return Err(failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no_replayable_steps",
            ));
        }
        if force_model.is_some() && !compare_models.is_empty() {
            return Err(failure(StatusCode::BAD_REQUEST, "conflicting_overrides"));
        }
        if compare_models.len() > 16 {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "too_many_comparison_models",
            ));
        }
        let models: Vec<Option<String>> = if compare_models.is_empty() {
            vec![force_model]
        } else {
            compare_models.into_iter().map(Some).collect()
        };
        for model in models.iter().flatten() {
            if !self.config.models.contains_key(model) {
                return Err(failure(StatusCode::BAD_REQUEST, "unknown_model"));
            }
        }
        let mut replays = Vec::new();
        for model in models {
            let id = self
                .store
                .lock()
                .await
                .create(json!({"source_trace_id":source_id}))
                .map_err(storage_error)?;
            let mut execution_error = None;
            for step in &steps {
                let mut input = step["input"].clone();
                if let Some(model) = &model {
                    input["model"] = json!(model);
                }
                if let Err(error) = self.execute(&id, input).await {
                    execution_error = Some(error);
                    break;
                }
            }
            self.store
                .lock()
                .await
                .complete(&id)
                .map_err(storage_error)?;
            if let Some(error) = execution_error {
                self.store
                    .lock()
                    .await
                    .fail(&id, &error.code)
                    .map_err(storage_error)?;
                return Err(Failure {
                    status: error.status,
                    code: format!("{} (replay trace: {})", error.code, id),
                });
            }
            replays.push(
                self.store
                    .lock()
                    .await
                    .trace(&id)
                    .map_err(storage_error)?
                    .unwrap(),
            );
        }
        Ok(json!({"status":"completed","source_trace_id":source_id,
        "comparison":{"source":source,"replays":replays}}))
    }
}

#[cfg(test)]
mod tests;
