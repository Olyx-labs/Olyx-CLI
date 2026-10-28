pub mod config;
mod diagnostics;
pub mod doctor;
mod embeddings;
mod plugin;
mod provider;
mod response;
pub mod server;
mod sse;
pub mod store;
mod trace_guard;
pub use trace_guard::wait_for_cancelled_traces;

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
    pub trace_id: Option<String>,
    pub diagnostic: Option<Box<diagnostics::Diagnostic>>,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(d) = &self.diagnostic {
            write!(
                f,
                ": {}\nHint: {}\nFallback: {}",
                d.message, d.hint, d.fallback
            )?;
        }
        if let Some(id) = &self.trace_id {
            write!(
                f,
                "\nTrace: {id}\nInspect: oy debug {id} (use the same --config file)"
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for Failure {}

pub struct State {
    config: Config,
    store: Arc<AsyncMutex<Store>>,
    client: reqwest::Client,
    provider_keys: std::collections::BTreeMap<String, String>,
}

fn failure(status: StatusCode, code: &str) -> Failure {
    Failure::new(status.as_u16(), code)
}

impl State {
    pub fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        config.validate()?;
        if let Some(parent) = std::path::Path::new(&config.database)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            #[cfg(unix)]
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
        Ok(Arc::new(Self {
            config,
            store: Arc::new(AsyncMutex::new(store)),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            provider_keys,
        }))
    }

    fn prepare(&self, mut body: Value) -> Result<Value, Failure> {
        if body.get("native_request").is_some() {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "native_requests_not_supported",
            ));
        }
        if !body.is_object() || body["messages"].as_array().is_none_or(|a| a.is_empty()) {
            return Err(failure(StatusCode::BAD_REQUEST, "messages_required"));
        }
        if body
            .get("stream")
            .is_some_and(|stream| !stream.is_boolean())
        {
            return Err(failure(StatusCode::BAD_REQUEST, "invalid_stream_option"));
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
        self.execute_events(trace_id, body, &mut |_| Ok(())).await
    }

    async fn execute_events(
        &self,
        trace_id: &str,
        body: Value,
        emit: &mut (dyn FnMut(Value) -> Result<(), Failure> + Send),
    ) -> Result<Value, Failure> {
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
                        &json!({"blocked":true,"reason":error.code,"request_sent":false}),
                        (0.0, 0, false),
                    )
                    .map_err(storage_error)?;
                return Err(error);
            }
        };
        let mut last_failure = None;
        for candidate in candidates {
            let mut payload = prepared.clone();
            payload["model"] = json!(candidate.model);
            if candidate.protocol == config::Protocol::Plugin {
                let started = Instant::now();
                let mut partial = Value::Null;
                let result = plugin::execute(candidate, "chat", &payload, self.provider_keys.get(&candidate.api_key_env).map(String::as_str), 8_388_608, &mut |output| {
                    partial=output.clone();
                    emit(json!({"type":"snapshot","trace_id":trace_id,"model":candidate.model,"output":output}))
                }).await;
                match result {
                    Ok(output) => {
                        let cost = (output["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as f64
                            * candidate.input_per_million
                            + output["usage"]["completion_tokens"].as_u64().unwrap_or(0) as f64
                                * candidate.output_per_million)
                            / 1_000_000.0;
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
                    Err(error) => {
                        self.store
                            .lock()
                            .await
                            .record(
                                trace_id,
                                &candidate.model,
                                &prepared,
                                &json!({"reason":error.code,"partial_output":partial}),
                                (0.0, started.elapsed().as_millis(), false),
                            )
                            .map_err(storage_error)?;
                        if partial.is_null()
                            && matches!(
                                error.code.as_str(),
                                "plugin_unavailable" | "plugin_rate_limited"
                            )
                        {
                            last_failure = Some(error);
                            continue;
                        }
                        return Err(error);
                    }
                }
            }
            let request = match self.provider_keys.get(&candidate.api_key_env) {
                Some(key) => provider::request(&self.client, candidate, &payload, key),
                None if candidate.api_key_env.is_empty() => {
                    provider::request(&self.client, candidate, &payload, "")
                }
                None => Err(diagnostics::missing_credential(candidate, &requested)),
            };
            let request = match request {
                Ok(request) => request,
                Err(error) => {
                    self.store
                        .lock()
                        .await
                        .record(
                            trace_id,
                            &candidate.model,
                            &prepared,
                            &json!({"reason":error.code,"request_sent":false}),
                            (0.0, 0, false),
                        )
                        .map_err(storage_error)?;
                    return Err(error);
                }
            };
            let output_limit = payload
                .get("max_completion_tokens")
                .or_else(|| payload.get("max_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(self.config.policy.max_output_tokens);
            // Local telemetry estimate when the provider does not report token usage.
            let estimated_cost = ((payload.to_string().len() + 4096) as f64
                * candidate.input_per_million
                + output_limit as f64 * candidate.output_per_million)
                / 1_000_000.0;
            let started = Instant::now();
            let response = request.send().await;
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    self.store.lock().await.record(trace_id,&candidate.model,&prepared,
                        &json!({"reason":"provider_transport_failure","provider":candidate.base_url}),(0.0, started.elapsed().as_millis(), false)).map_err(storage_error)?;
                    last_failure = Some(diagnostics::provider_failure(candidate, &requested, None));
                    continue;
                }
            };
            if !response.status().is_success() {
                self.store.lock().await.record(trace_id,&candidate.model,&prepared,
                    &json!({"reason":"provider_rejected_request","status":response.status().as_u16(),"provider":candidate.base_url}),(0.0, started.elapsed().as_millis(), false)).map_err(storage_error)?;
                let retry =
                    response.status().is_server_error() || response.status().as_u16() == 429;
                let error = diagnostics::provider_failure(
                    candidate,
                    &requested,
                    Some(response.status().as_u16()),
                );
                if retry {
                    last_failure = Some(error);
                    continue;
                }
                return Err(error);
            }
            let streaming = prepared["stream"].as_bool() == Some(true);
            let mut partial = Value::Null;
            let output = match provider::read(response, streaming, &mut |output| {
                partial = output.clone();
                emit(json!({"type":"snapshot","trace_id":trace_id,"model":candidate.model,"output":output}))
            }).await {
                Ok(output) => output,
                Err(error) => {
                    self.store.lock().await.record(trace_id, &candidate.model, &prepared,
                        &json!({"reason":error.code,"partial_output":partial}), (estimated_cost, started.elapsed().as_millis(), false)).map_err(storage_error)?;
                    // Never fall back after a successful HTTP response: streaming may
                    // already have delivered content or incurred upstream usage.
                    return Err(diagnostics::response_failure(error, candidate, &requested));
                }
            };
            let cost = match (
                output["usage"]["prompt_tokens"].as_u64(),
                output["usage"]["completion_tokens"].as_u64(),
            ) {
                (Some(input), Some(output)) => {
                    (input as f64 * candidate.input_per_million
                        + output as f64 * candidate.output_per_million)
                        / 1_000_000.0
                }
                _ => estimated_cost,
            };
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
        Err(last_failure
            .unwrap_or_else(|| failure(StatusCode::BAD_GATEWAY, "all_providers_unavailable")))
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
        self.route_with_events(body, &mut |_| Ok(())).await
    }

    pub async fn route_with_events(
        &self,
        body: Value,
        emit: &mut (dyn FnMut(Value) -> Result<(), Failure> + Send),
    ) -> Result<Value, Failure> {
        let id = self
            .store
            .lock()
            .await
            .create(json!({"source":"olyx-cli"}))
            .map_err(storage_error)?;
        let mut guard = trace_guard::TraceGuard::new(self.store.clone(), &id);
        let output = self.execute_events(&id, body, emit).await;
        self.finish_trace(&id, &output).await?;
        guard.disarm();
        output
            .map(|output| json!({"trace_id":id,"output":output}))
            .map_err(|error| error.with_trace(id))
    }

    /// Persist a terminal result; the caller disarms its cancellation guard only
    /// after this succeeds, including when the provider operation itself failed.
    pub(crate) async fn finish_trace(
        &self,
        id: &str,
        result: &Result<Value, Failure>,
    ) -> Result<(), Failure> {
        let store = self.store.lock().await;
        match result {
            Ok(_) => {
                store.complete(id).map_err(storage_error)?;
            }
            Err(error) => store
                .fail_details(id, &error.with_trace_ref(id))
                .map_err(storage_error)?,
        }
        Ok(())
    }

    pub async fn healthy(&self) -> Result<(), Failure> {
        self.store
            .lock()
            .await
            .0
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .map(|_| ())
            .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "local_storage_unavailable"))
    }

    pub async fn debug(&self, id: Option<&str>) -> Result<Value, Failure> {
        let store = self.store.lock().await;
        store
            .expire(self.config.policy.retention_days)
            .map_err(storage_error)?;
        match id {
            Some(id) => {
                let mut trace = store
                    .trace(id)
                    .map_err(storage_error)?
                    .ok_or_else(|| failure(StatusCode::NOT_FOUND, "trace_not_found"))?;
                if trace["status"] == "failed" {
                    trace["diagnostic"] = trace["metadata"]["diagnostic"].clone();
                    if trace["diagnostic"].is_null() {
                        trace["diagnostic"] = json!({"code":trace["metadata"]["error"],"trace_id":id,"message":"Inspect failed provider attempts in steps. This trace predates structured diagnostics."});
                    }
                }
                Ok(trace)
            }
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
        let comparing = !compare_models.is_empty();
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
                .create(
                    json!({"source_trace_id":source_id,"requested_model":model,"endpoint":source["metadata"]["endpoint"]}),
                )
                .map_err(storage_error)?;
            let mut guard = trace_guard::TraceGuard::new(self.store.clone(), &id);
            let mut execution_error = None;
            for step in &steps {
                let mut input = step["input"].clone();
                if let Some(model) = &model {
                    input["model"] = json!(model);
                }
                let result = if source["metadata"]["endpoint"] == "/v1/embeddings" {
                    self.execute_embeddings(&id, input).await
                } else {
                    self.execute(&id, input).await
                };
                if let Err(error) = result {
                    execution_error = Some(error);
                    break;
                }
            }
            if let Some(error) = execution_error {
                self.store
                    .lock()
                    .await
                    .fail_details(&id, &error.with_trace_ref(&id))
                    .map_err(storage_error)?;
                if !comparing {
                    guard.disarm();
                    return Err(error.with_trace(id));
                }
            } else {
                self.store
                    .lock()
                    .await
                    .complete(&id)
                    .map_err(storage_error)?;
            }
            guard.disarm();
            replays.push(
                self.store
                    .lock()
                    .await
                    .trace(&id)
                    .map_err(storage_error)?
                    .unwrap(),
            );
        }
        let failed = replays
            .iter()
            .filter(|trace| trace["status"] == "failed")
            .count();
        let status = if failed == 0 {
            "completed"
        } else if failed == replays.len() {
            "failed"
        } else {
            "partial"
        };
        Ok(json!({"status":status,"source_trace_id":source_id,
        "comparison":{"source":source,"succeeded":replays.len()-failed,"failed":failed,"replays":replays}}))
    }
}

#[cfg(test)]
mod tests;
