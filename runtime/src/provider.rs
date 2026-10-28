//! OpenAI-compatible HTTP transport. Native APIs belong in external plugins.
use crate::{Failure, config::Provider, failure};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde_json::{Value, json};
fn upstream(code: &str) -> Failure {
    failure(StatusCode::BAD_GATEWAY, code)
}
pub fn request(
    client: &Client,
    provider: &Provider,
    body: &Value,
    key: &str,
) -> Result<RequestBuilder, Failure> {
    let mut payload = body.clone();
    if payload.get("native_request").is_some() {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "native_requests_not_supported",
        ));
    }
    payload["model"] = json!(provider.model);
    if payload.get("max_completion_tokens").is_some() {
        payload.as_object_mut().unwrap().remove("max_tokens");
    }
    if payload["stream"] == true && payload.get("stream_options").is_none() {
        payload["stream_options"] = json!({"include_usage":true});
    }
    let builder = client
        .post(format!(
            "{}/chat/completions",
            provider.base_url.trim_end_matches('/')
        ))
        .json(&payload);
    let builder = if key.is_empty() {
        builder
    } else {
        builder.bearer_auth(key)
    };
    Ok(if payload["stream"] == true {
        builder
    } else {
        builder.timeout(std::time::Duration::from_secs(60))
    })
}
fn with_total_usage(mut output: Value) -> Value {
    if !output["usage"]["total_tokens"].is_null() {
        return output;
    }
    if let Some(total) = output["usage"]["prompt_tokens"]
        .as_u64()
        .zip(output["usage"]["completion_tokens"].as_u64())
        .and_then(|(input, output)| input.checked_add(output))
    {
        output["usage"]["total_tokens"] = json!(total);
    }
    output
}

fn normalized(native: Value, complete: bool) -> Result<Value, Failure> {
    if native["choices"].as_array().is_none_or(|choices| {
        choices.is_empty()
            || choices.iter().any(|choice| {
                let message = &choice["message"];
                !message.is_object()
                    || !(message.get("content").is_some_and(|content| {
                        content.is_null() || content.is_string() || content.is_array()
                    }) || message["refusal"].is_string()
                        || message["tool_calls"]
                            .as_array()
                            .is_some_and(|calls| !calls.is_empty())
                        || message["function_call"].is_object())
            })
    }) {
        return Err(upstream("invalid_provider_response"));
    }
    crate::response::checked(with_total_usage(native), complete)
}
struct Stream {
    output: Value,
    done: bool,
}
impl Stream {
    fn new() -> Self {
        Self {
            output: json!({"choices":[],"usage":{}}),
            done: false,
        }
    }
    fn snapshot(&self) -> Result<Value, Failure> {
        normalized(self.output.clone(), false)
    }
    fn emit_events(
        &mut self,
        events: Vec<String>,
        emit: &mut (dyn FnMut(Value) -> Result<(), Failure> + Send),
    ) -> Result<(), Failure> {
        for data in events {
            if data.is_empty() || !self.event(&data)? {
                continue;
            }
            if let Ok(snapshot) = self.snapshot() {
                emit(snapshot)?;
            }
        }
        Ok(())
    }
    fn event(&mut self, data: &str) -> Result<bool, Failure> {
        if data == "[DONE]" {
            self.done = true;
            return Ok(false);
        }
        let v: Value = serde_json::from_str(data).map_err(|_| upstream("invalid_stream_event"))?;
        if v.get("error").is_some_and(|v| !v.is_null()) {
            return Err(upstream("provider_stream_error"));
        }

        for (key, value) in v
            .as_object()
            .ok_or_else(|| upstream("invalid_stream_event"))?
        {
            if key != "choices" && !value.is_null() {
                self.output[key] = value.clone();
            }
        }
        if let Some(choices) = v["choices"].as_array() {
            for choice in choices {
                let index = choice["index"].as_u64().unwrap_or(0) as usize;
                if index > 128 {
                    return Err(upstream("invalid_stream_index"));
                }
                let target = self.output["choices"].as_array_mut().unwrap();
                while target.len() <= index {
                    target.push(json!({"index":target.len(),"message":{"role":"assistant","content":""},"finish_reason":null}));
                }
                let target = &mut target[index];
                if !choice["finish_reason"].is_null() {
                    target["finish_reason"] = choice["finish_reason"].clone();
                }
                if let Some(delta) = choice["delta"].as_object() {
                    for (key, value) in delta {
                        if key == "tool_calls" {
                            if target["message"].get("tool_calls").is_none() {
                                target["message"]["tool_calls"] = json!([]);
                            }
                            for tool in value
                                .as_array()
                                .ok_or_else(|| upstream("invalid_tool_delta"))?
                            {
                                let n = tool["index"].as_u64().unwrap_or(0) as usize;
                                if n > 128 {
                                    return Err(upstream("invalid_stream_index"));
                                }
                                let tools = target["message"]["tool_calls"].as_array_mut().unwrap();
                                while tools.len() <= n {
                                    tools.push(json!({"type":"function","function":{"name":"","arguments":""}}));
                                }
                                for key in ["id", "type"] {
                                    if tool.get(key).is_some() {
                                        tools[n][key] = tool[key].clone();
                                    }
                                }
                                for key in ["name", "arguments"] {
                                    if tool["function"].get(key).is_some_and(|v| !v.is_string()) {
                                        return Err(upstream("invalid_tool_delta"));
                                    }
                                    append(&mut tools[n]["function"][key], &tool["function"][key]);
                                }
                            }
                        } else if key == "role" {
                            target["message"][key] = value.clone();
                        } else if value.is_string() {
                            append(&mut target["message"][key], value);
                        } else if !value.is_null() {
                            target["message"][key] = value.clone();
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
fn append(target: &mut Value, delta: &Value) {
    if let Some(delta) = delta.as_str() {
        if let Value::String(text) = target {
            text.push_str(delta);
        } else {
            *target = Value::String(delta.to_owned());
        }
    }
}
pub async fn read(
    mut response: Response,
    streaming: bool,
    emit: &mut (dyn FnMut(Value) -> Result<(), Failure> + Send),
) -> Result<Value, Failure> {
    let mut pending = Vec::new();
    let mut size = 0usize;
    let mut stream = Stream::new();
    let mut decoder = crate::sse::Decoder::default();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| upstream("provider_stream_interrupted"))?
    {
        if chunk.len() > 8_388_608usize.saturating_sub(size) {
            return Err(upstream("provider_response_too_large"));
        }
        size += chunk.len();
        if streaming {
            stream.emit_events(decoder.feed(&chunk, false)?, emit)?;
        } else {
            pending.extend_from_slice(&chunk);
        }
    }
    if streaming {
        stream.emit_events(decoder.feed(&[], true)?, emit)?;
        if !stream.done
            || (stream.output["choices"].as_array().is_none_or(|choices| {
                choices.is_empty()
                    || choices
                        .iter()
                        .any(|choice| choice["finish_reason"].as_str().is_none_or(str::is_empty))
            }))
        {
            return Err(upstream("provider_stream_incomplete"));
        }
        normalized(stream.output, true)
    } else {
        let native =
            serde_json::from_slice(&pending).map_err(|_| upstream("invalid_provider_response"))?;
        normalized(native, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_events_preserve_snapshots_and_propagate_output_errors() {
        let events = vec![
            String::new(),
            json!({"choices":[{"index":0,"delta":{"content":"hé"}}]}).to_string(),
            json!({"choices":[{"index":0,"delta":{"content":"llo"},"finish_reason":"stop"}]})
                .to_string(),
            "[DONE]".into(),
        ];
        let mut snapshots = Vec::new();
        let mut stream = Stream::new();
        stream
            .emit_events(events.clone(), &mut |value| {
                snapshots.push(value);
                Ok(())
            })
            .unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0]["choices"][0]["message"]["content"], "hé");
        assert_eq!(snapshots[1]["choices"][0]["message"]["content"], "héllo");
        assert!(stream.done);
        let error = Stream::new()
            .emit_events(events, &mut |_| Err(upstream("output_write_failed")))
            .unwrap_err();
        assert_eq!(error.code, "output_write_failed");
    }
    #[test]
    fn decoded_provider_errors_are_not_emitted_as_success() {
        let mut emitted = false;
        let error = Stream::new()
            .emit_events(
                vec![r#"{"error":{"message":"failed"}}"#.into()],
                &mut |_| {
                    emitted = true;
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "provider_stream_error");
        assert!(!emitted);
    }
    #[test]
    fn compatible_streams_preserve_provider_metadata() {
        let mut stream = Stream::new();
        stream.event(r#"{"citations":["https://example.com"],"search_results":[{"title":"Source"}],"choices":[{"index":0,"delta":{"content":"Answer","reasoning_content":"reason"},"finish_reason":"stop"}]}"#).unwrap();
        stream.event("[DONE]").unwrap();
        let output = stream.snapshot().unwrap();
        assert_eq!(output["citations"][0], "https://example.com");
        assert_eq!(
            output["choices"][0]["message"]["reasoning_content"],
            "reason"
        );
    }
    #[test]
    fn openai_tool_deltas_are_accumulated() {
        let mut stream = Stream::new();
        stream.event(&json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"weather","arguments":"{\"city\":"}}]}}]}).to_string()).unwrap();
        stream.event(&json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Toronto\"}"}}]}}]}).to_string()).unwrap();
        stream.event("[DONE]").unwrap();
        let output = stream.snapshot().unwrap();
        assert_eq!(
            output["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"Toronto\"}"
        );
    }
    #[test]
    fn completion_token_limit_takes_precedence_without_conflicting_wire_fields() {
        let provider:Provider=serde_json::from_value(json!({"base_url":"https://api.openai.com/v1","model":"configured","api_key_env":"KEY"})).unwrap();
        let body = json!({"model":"alias","messages":[{"role":"user","content":"Hi"}],"max_tokens":128,"max_completion_tokens":2});
        let request = request(&Client::new(), &provider, &body, "test-key")
            .unwrap()
            .build()
            .unwrap();
        let wire: Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(wire["max_completion_tokens"], 2);
        assert!(wire.get("max_tokens").is_none());
        assert_eq!(body["max_tokens"], 128);
    }
}
