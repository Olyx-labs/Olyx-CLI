//! Text embeddings for application-owned vector indexing and retrieval.
use crate::{Failure, State, config::Protocol, failure, storage_error};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::time::Instant;

fn invalid(code: &str) -> Failure {
    failure(StatusCode::BAD_REQUEST, code)
}
fn upstream(code: &str) -> Failure {
    failure(StatusCode::BAD_GATEWAY, code)
}
fn token_array(value: &Value) -> bool {
    value.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.iter()
                .all(|v| v.as_u64().is_some_and(|n| n <= u32::MAX as u64))
    })
}
fn input_count(body: &Value) -> Result<usize, Failure> {
    match &body["input"] {
        Value::String(s) if !s.is_empty() => Ok(1),
        Value::Array(items) if !items.is_empty() => {
            if token_array(&body["input"]) {
                return Ok(1);
            }
            if items
                .iter()
                .all(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                || items.iter().all(token_array)
            {
                return Ok(items.len());
            }
            Err(invalid("invalid_embedding_input"))
        }
        _ => Err(invalid("invalid_embedding_input")),
    }
}
fn validate(body: &Value) -> Result<usize, Failure> {
    if !body.is_object() {
        return Err(invalid("request_must_be_object"));
    }
    if body["model"].as_str().is_none_or(str::is_empty) {
        return Err(invalid("model_required"));
    }
    if body
        .get("encoding_format")
        .is_some_and(|v| v != "float" && v != "base64")
    {
        return Err(invalid("invalid_embedding_encoding"));
    }
    if body
        .get("dimensions")
        .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0 || n > u32::MAX as u64))
    {
        return Err(invalid("invalid_embedding_dimensions"));
    }
    if body.get("user").is_some_and(|v| !v.is_string()) {
        return Err(invalid("invalid_embedding_user"));
    }
    if ["stream", "messages", "native_request"]
        .iter()
        .any(|key| body.get(key).is_some())
    {
        return Err(invalid("unsupported_embedding_option"));
    }
    input_count(body)
}
fn dimensions(value: &Value, encoding: &str) -> Result<usize, Failure> {
    if encoding == "base64" {
        let bytes = STANDARD
            .decode(
                value
                    .as_str()
                    .ok_or_else(|| upstream("invalid_embedding_response"))?,
            )
            .map_err(|_| upstream("invalid_embedding_response"))?;
        if bytes.is_empty()
            || bytes.len() % 4 != 0
            || bytes
                .as_chunks::<4>()
                .0
                .iter()
                .any(|b| !f32::from_le_bytes(*b).is_finite())
        {
            return Err(upstream("invalid_embedding_response"));
        }
        Ok(bytes.len() / 4)
    } else {
        let values = value
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| upstream("invalid_embedding_response"))?;
        if values
            .iter()
            .any(|v| v.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err(upstream("invalid_embedding_response"));
        }
        Ok(values.len())
    }
}
fn normalize(
    mut output: Value,
    input: &Value,
    count: usize,
    model: &str,
    response_models: &[String],
) -> Result<(Value, Value), Failure> {
    crate::response::usage(&output["usage"])?;
    let entries = output["data"]
        .as_array()
        .filter(|a| a.len() == count)
        .ok_or_else(|| upstream("invalid_embedding_response"))?;
    let encoding = input["encoding_format"].as_str().unwrap_or("float");
    let mut seen = vec![false; count];
    let mut width = None;
    for entry in entries {
        let index = entry["index"]
            .as_u64()
            .filter(|n| *n < count as u64)
            .ok_or_else(|| upstream("invalid_embedding_response"))? as usize;
        if seen[index] {
            return Err(upstream("invalid_embedding_response"));
        }
        seen[index] = true;
        let size = dimensions(&entry["embedding"], encoding)?;
        if width.is_some_and(|previous| previous != size)
            || input["dimensions"]
                .as_u64()
                .is_some_and(|n| n != size as u64)
        {
            return Err(upstream("embedding_dimension_mismatch"));
        }
        width = Some(size);
    }
    output["object"] = json!("list");
    let reported = output["model"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| upstream("invalid_embedding_response"))?;
    if reported != model && !response_models.iter().any(|name| name == reported) {
        return Err(upstream("embedding_model_mismatch"));
    }
    for entry in output["data"].as_array_mut().unwrap() {
        entry["object"] = json!("embedding");
    }
    output["data"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|v| v["index"].as_u64().unwrap());
    // Vectors can dwarf the source text. Traces retain replay input and response
    // metrics, while the caller receives the actual vectors for its own index.
    let summary = json!({"object":"embedding_summary","model":output["model"],"embedding_count":count,"dimensions":width,"encoding_format":encoding,"usage":output["usage"]});
    Ok((output, summary))
}
impl State {
    pub async fn embeddings(&self, body: Value) -> Result<Value, Failure> {
        validate(&body)?;
        let id = self
            .store
            .lock()
            .await
            .create(json!({"source":"olyx-cli","endpoint":"/v1/embeddings"}))
            .map_err(storage_error)?;
        let mut guard = crate::trace_guard::TraceGuard::new(self.store.clone(), &id);
        let result = self.execute_embeddings(&id, body).await;
        self.finish_trace(&id, &result).await?;
        guard.disarm();
        result
            .map(|output| json!({"trace_id":id,"output":output}))
            .map_err(|error| error.with_trace(id))
    }
    pub(crate) async fn execute_embeddings(&self, id: &str, body: Value) -> Result<Value, Failure> {
        let count = validate(&body)?;
        if body.to_string().len() > self.config.policy.max_body_bytes {
            return Err(failure(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large"));
        }
        let alias = body["model"].as_str().unwrap();
        let candidates = self
            .config
            .models
            .get(alias)
            .ok_or_else(|| invalid("unknown_model"))?;
        let first = &candidates[0];
        if candidates.iter().any(|p| {
            p.model != first.model
                || p.protocol != first.protocol
                || p.embedding_response_models
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    != first
                        .embedding_response_models
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
        }) {
            return Err(invalid("embedding_fallback_model_mismatch"));
        }
        let mut last_failure = None;
        for candidate in candidates {
            let started = Instant::now();
            if candidate.protocol == Protocol::Plugin {
                let mut payload = body.clone();
                payload["model"] = json!(candidate.model);
                let result = crate::plugin::execute(
                    candidate,
                    "embeddings",
                    &payload,
                    self.provider_keys
                        .get(&candidate.api_key_env)
                        .map(String::as_str),
                    self.config.policy.max_embedding_response_bytes,
                    &mut |_| Ok(()),
                )
                .await;
                let result = result.and_then(|output| {
                    normalize(
                        output,
                        &body,
                        count,
                        &candidate.model,
                        &candidate.embedding_response_models,
                    )
                });
                match result {
                    Ok((output, summary)) => {
                        let cost = output["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as f64
                            * candidate.input_per_million
                            / 1_000_000.0;
                        self.store
                            .lock()
                            .await
                            .record(
                                id,
                                &candidate.model,
                                &body,
                                &summary,
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
                                id,
                                &candidate.model,
                                &body,
                                &json!({"reason":error.code}),
                                (0.0, started.elapsed().as_millis(), false),
                            )
                            .map_err(storage_error)?;
                        if matches!(
                            error.code.as_str(),
                            "plugin_unavailable" | "plugin_rate_limited"
                        ) {
                            last_failure = Some(error);
                            continue;
                        }
                        return Err(error);
                    }
                }
            }
            let key = match self.provider_keys.get(&candidate.api_key_env) {
                Some(key) => key,
                None if candidate.api_key_env.is_empty() => "",
                None => {
                    let error = crate::diagnostics::missing_credential(candidate, alias);
                    self.store
                        .lock()
                        .await
                        .record(
                            id,
                            &candidate.model,
                            &body,
                            &json!({"reason":error.code,"request_sent":false}),
                            (0.0, 0, false),
                        )
                        .map_err(storage_error)?;
                    return Err(error);
                }
            };
            let mut payload = body.clone();
            payload["model"] = json!(candidate.model);
            let builder = self.client.post(format!(
                "{}/embeddings",
                candidate.base_url.trim_end_matches('/')
            ));
            let builder = if key.is_empty() {
                builder
            } else {
                builder.bearer_auth(key)
            };
            let response = builder.json(&payload).send().await;
            let mut response = match response {
                Ok(r) => r,
                Err(_) => {
                    self.store.lock().await.record(id,&candidate.model,&body,&json!({"reason":"provider_transport_failure","provider":candidate.base_url}),(0.0,started.elapsed().as_millis(),false)).map_err(storage_error)?;
                    last_failure =
                        Some(crate::diagnostics::provider_failure(candidate, alias, None));
                    continue;
                }
            };
            if !response.status().is_success() {
                let status = response.status();
                self.store.lock().await.record(id,&candidate.model,&body,&json!({"reason":"provider_rejected_request","status":status.as_u16(),"provider":candidate.base_url}),(0.0,started.elapsed().as_millis(),false)).map_err(storage_error)?;
                let error =
                    crate::diagnostics::provider_failure(candidate, alias, Some(status.as_u16()));
                if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                    last_failure = Some(error);
                    continue;
                }
                return Err(error);
            }
            let result = async {
                let limit = self.config.policy.max_embedding_response_bytes;
                if response.content_length().is_some_and(|n| n > limit as u64) {
                    return Err(upstream("embedding_response_too_large"));
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|_| upstream("provider_response_interrupted"))?
                {
                    if chunk.len() > limit.saturating_sub(bytes.len()) {
                        return Err(upstream("embedding_response_too_large"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let output = serde_json::from_slice(&bytes)
                    .map_err(|_| upstream("invalid_embedding_response"))?;
                normalize(
                    output,
                    &body,
                    count,
                    &candidate.model,
                    &candidate.embedding_response_models,
                )
            }
            .await;
            match result {
                Ok((output, summary)) => {
                    let cost = output["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as f64
                        * candidate.input_per_million
                        / 1_000_000.0;
                    self.store
                        .lock()
                        .await
                        .record(
                            id,
                            &candidate.model,
                            &body,
                            &summary,
                            (cost, started.elapsed().as_millis(), true),
                        )
                        .map_err(storage_error)?;
                    return Ok(output);
                }
                Err(e) => {
                    self.store
                        .lock()
                        .await
                        .record(
                            id,
                            &candidate.model,
                            &body,
                            &json!({"reason":e.code}),
                            (0.0, started.elapsed().as_millis(), false),
                        )
                        .map_err(storage_error)?;
                    return Err(crate::diagnostics::response_failure(e, candidate, alias));
                }
            }
        }
        Err(last_failure.unwrap_or_else(|| upstream("all_providers_failed")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedding_model_identity_is_strict_with_explicit_version_aliases() {
        let body = json!({"input":"text"});
        let output = json!({"model":"model-v2","data":[{"index":0,"embedding":[1.0,2.0]}]});
        assert_eq!(
            normalize(output.clone(), &body, 1, "deployment", &[])
                .unwrap_err()
                .code,
            "embedding_model_mismatch"
        );
        let (output, summary) =
            normalize(output, &body, 1, "deployment", &["model-v2".into()]).unwrap();
        assert_eq!(output["model"], "model-v2");
        assert_eq!(summary["model"], "model-v2");
        assert!(
            normalize(
                json!({"data":[{"index":0,"embedding":[1.0,2.0]}]}),
                &body,
                1,
                "deployment",
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn text_and_token_batches_are_counted_correctly() {
        for (input, count) in [
            (json!("document"), 1),
            (json!(["one", "two"]), 2),
            (json!([1, 2, 3]), 1),
            (json!([[1], [2, 3]]), 2),
        ] {
            assert_eq!(
                validate(&json!({"model":"embed","input":input})).unwrap(),
                count
            );
        }
        for input in [
            Value::Null,
            json!(""),
            json!([]),
            json!(["one", 1]),
            json!([[]]),
            json!([-1]),
            json!([1.5]),
        ] {
            assert!(validate(&json!({"model":"embed","input":input})).is_err());
        }
    }
    #[test]
    fn vectors_are_ordered_and_traces_store_metadata() {
        let body = json!({"model":"embed","input":["a","b"],"dimensions":2});
        let response = json!({"model":"model-v1","data":[{"index":1,"embedding":[0.3,0.4]},{"index":0,"embedding":[0.1,0.2]}],"usage":{"prompt_tokens":2,"total_tokens":2}});
        let (output, summary) = normalize(response.clone(), &body, 2, "model-v1", &[]).unwrap();
        assert_eq!(output["data"][0]["index"], 0);
        assert_eq!(summary["embedding_count"], 2);
        assert!(summary.get("data").is_none());
        let mut duplicate = response.clone();
        duplicate["data"][1]["index"] = json!(1);
        assert!(normalize(duplicate, &body, 2, "x", &[]).is_err());
        assert!(normalize(response, &json!({"dimensions":3}), 2, "x", &[]).is_err());
    }
    #[test]
    fn base64_embeddings_are_valid_float32_vectors() {
        let encoded = STANDARD.encode([1.0f32.to_le_bytes(), 2.0f32.to_le_bytes()].concat());
        assert_eq!(dimensions(&json!(encoded), "base64").unwrap(), 2);
        assert!(dimensions(&json!("bad!"), "base64").is_err());
        assert!(dimensions(&json!(STANDARD.encode(f32::NAN.to_le_bytes())), "base64").is_err());
    }
}
