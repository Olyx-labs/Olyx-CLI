//! Provider wire protocols. Requests use a common chat envelope; native_request
//! supplies a provider-native body when advanced or multimodal features are needed.
use crate::{
    Failure,
    config::{Protocol, Provider},
    failure,
};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde_json::{Value, json};

fn invalid(code: &str) -> Failure {
    failure(StatusCode::BAD_REQUEST, code)
}
fn upstream(code: &str) -> Failure {
    failure(StatusCode::BAD_GATEWAY, code)
}

pub fn request(
    client: &Client,
    provider: &Provider,
    body: &Value,
    key: &str,
) -> Result<RequestBuilder, Failure> {
    let stream = body["stream"].as_bool() == Some(true);
    let tokens = body
        .get("max_completion_tokens")
        .or_else(|| body.get("max_tokens"))
        .cloned()
        .unwrap_or(json!(1024));
    let base = provider.base_url.trim_end_matches('/');
    let mut native = if let Some(native) = body.get("native_request") {
        if !native.is_object() {
            return Err(invalid("native_request_must_be_object"));
        }
        native.clone()
    } else if provider.protocol == Protocol::Openai {
        body.clone()
    } else {
        // Do not silently discard options that change the requested generation.
        for field in body
            .as_object()
            .ok_or_else(|| invalid("invalid_request"))?
            .keys()
        {
            if ![
                "model",
                "messages",
                "stream",
                "max_tokens",
                "max_completion_tokens",
                "temperature",
                "top_p",
                "stop",
            ]
            .contains(&field.as_str())
            {
                return Err(invalid("native_option_requires_native_request"));
            }
        }
        let messages = body["messages"]
            .as_array()
            .ok_or_else(|| invalid("messages_required"))?;
        let mut system = Vec::new();
        let mut conversation = Vec::new();
        for message in messages {
            let role = message["role"]
                .as_str()
                .ok_or_else(|| invalid("message_role_required"))?;
            let text = message["content"]
                .as_str()
                .ok_or_else(|| invalid("native_content_requires_native_request"))?;
            match role {
                "system" | "developer" => system.push(text),
                "user" | "assistant" => conversation.push(match provider.protocol {
                    Protocol::Anthropic => json!({"role":role,"content":text}),
                    _ => json!({"role":if role == "assistant" {"model"} else {"user"},"parts":[{"text":text}]}),
                }),
                _ => return Err(invalid("native_role_requires_native_request")),
            }
        }
        if conversation.is_empty() {
            return Err(invalid("messages_required"));
        }
        match provider.protocol {
            Protocol::Anthropic => {
                let mut value = json!({"messages":conversation});
                if !system.is_empty() {
                    value["system"] = json!(system.join("\n\n"));
                }
                for name in ["temperature", "top_p"] {
                    if let Some(v) = body.get(name) {
                        value[name] = v.clone();
                    }
                }
                if let Some(stop) = body.get("stop") {
                    value["stop_sequences"] = if stop.is_string() {
                        json!([stop])
                    } else {
                        stop.clone()
                    };
                }
                value
            }
            _ => {
                let mut value = json!({"contents":conversation,"generationConfig":{}});
                if !system.is_empty() {
                    value["systemInstruction"] = json!({"parts":[{"text":system.join("\n\n")}]});
                }
                for (source, target) in [
                    ("temperature", "temperature"),
                    ("top_p", "topP"),
                    ("stop", "stopSequences"),
                ] {
                    if let Some(v) = body.get(source) {
                        value["generationConfig"][target] = if source == "stop" && v.is_string() {
                            json!([v])
                        } else {
                            v.clone()
                        };
                    }
                }
                value
            }
        }
    };
    native.as_object_mut().unwrap().remove("native_request");
    let builder = match provider.protocol {
        Protocol::Openai => {
            native["model"] = json!(provider.model);
            native["stream"] = json!(stream);
            native["max_tokens"] = tokens.clone();
            native
                .as_object_mut()
                .unwrap()
                .remove("max_completion_tokens");
            if body.get("max_completion_tokens").is_some() {
                native.as_object_mut().unwrap().remove("max_tokens");
                native["max_completion_tokens"] = tokens;
            }
            if stream && native.get("stream_options").is_none() {
                native["stream_options"] = json!({"include_usage":true});
            }
            client
                .post(format!("{base}/chat/completions"))
                .bearer_auth(key)
        }
        Protocol::Anthropic => {
            native["model"] = json!(provider.model);
            native["stream"] = json!(stream);
            native["max_tokens"] = tokens;
            client
                .post(format!("{base}/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        }
        Protocol::Gemini => {
            if native.get("generationConfig").is_none() {
                native["generationConfig"] = json!({});
            }
            if !native["generationConfig"].is_object() {
                return Err(invalid("invalid_generation_config"));
            }
            native["generationConfig"]["maxOutputTokens"] = tokens;
            let mut url = reqwest::Url::parse(&format!("{base}/models/"))
                .map_err(|_| invalid("invalid_provider_url"))?;
            url.path_segments_mut()
                .map_err(|_| invalid("invalid_provider_url"))?
                .pop_if_empty()
                .push(&format!(
                    "{}:{}",
                    provider.model,
                    if stream {
                        "streamGenerateContent"
                    } else {
                        "generateContent"
                    }
                ));
            if stream {
                url.query_pairs_mut().append_pair("alt", "sse");
            }
            client.post(url).header("x-goog-api-key", key)
        }
    };
    let builder = builder.json(&native);
    Ok(if stream {
        builder
    } else {
        builder.timeout(std::time::Duration::from_secs(60))
    })
}

fn normalized(native: Value, protocol: Protocol) -> Result<Value, Failure> {
    if protocol == Protocol::Openai {
        if native["choices"].as_array().is_none_or(|v| v.is_empty()) {
            return Err(upstream("invalid_provider_response"));
        }
        return Ok(native);
    }
    let (blocks, input, output, finish) = match protocol {
        Protocol::Anthropic => (
            native["content"].as_array(),
            native["usage"]["input_tokens"].clone(),
            native["usage"]["output_tokens"].clone(),
            native["stop_reason"].clone(),
        ),
        _ => (
            native["candidates"][0]["content"]["parts"].as_array(),
            native["usageMetadata"]["promptTokenCount"].clone(),
            native["usageMetadata"]["candidatesTokenCount"].clone(),
            native["candidates"][0]["finishReason"].clone(),
        ),
    };
    let blocks = blocks.ok_or_else(|| upstream("invalid_provider_response"))?;
    let mut text = String::new();
    let mut tools = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if let Some(part) = block["text"].as_str() {
            if block["thought"] != true {
                text.push_str(part);
            }
        }
        if block["type"] == "tool_use" {
            tools.push(json!({"id":block["id"],"type":"function","function":{"name":block["name"],"arguments":block["input"].to_string()}}));
        } else if let Some(call) = block.get("functionCall") {
            tools.push(json!({"id":call.get("id").cloned().unwrap_or(json!(format!("call_{index}"))),"type":"function","function":{"name":call["name"],"arguments":call["args"].to_string()}}));
        }
    }
    let mut message = json!({"role":"assistant","content":text});
    if !tools.is_empty() {
        message["tool_calls"] = json!(tools);
    }
    Ok(
        json!({"id":native.get("id").or_else(|| native.get("responseId")),"model":native.get("model").or_else(|| native.get("modelVersion")),"choices":[{"index":0,"message":message,"finish_reason":finish}],"usage":{"prompt_tokens":input,"completion_tokens":output},"native_response":native}),
    )
}

struct Stream {
    protocol: Protocol,
    output: Value,
    done: bool,
    tool_json: std::collections::BTreeMap<usize, String>,
}
impl Stream {
    fn new(protocol: Protocol) -> Self {
        let output = match protocol {
            Protocol::Openai => json!({"choices":[],"usage":{}}),
            Protocol::Anthropic => json!({"content":[],"usage":{}}),
            Protocol::Gemini => json!({"candidates":[{"content":{"parts":[]}}]}),
        };
        Self {
            protocol,
            output,
            done: false,
            tool_json: Default::default(),
        }
    }
    fn snapshot(&self) -> Result<Value, Failure> {
        normalized(self.output.clone(), self.protocol)
    }
    fn event(&mut self, data: &str) -> Result<bool, Failure> {
        if data == "[DONE]" && self.protocol == Protocol::Openai {
            self.done = true;
            return Ok(false);
        }
        let v: Value = serde_json::from_str(data).map_err(|_| upstream("invalid_stream_event"))?;
        if v.get("error").is_some() || v["type"] == "error" {
            return Err(upstream("provider_stream_error"));
        }
        match self.protocol {
            Protocol::Openai => {
                for key in ["id", "model", "usage"] {
                    if v.get(key).is_some_and(|v| !v.is_null()) {
                        self.output[key] = v[key].clone();
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
                                        let tools =
                                            target["message"]["tool_calls"].as_array_mut().unwrap();
                                        while tools.len() <= n {
                                            tools.push(json!({"type":"function","function":{"name":"","arguments":""}}));
                                        }
                                        for key in ["id", "type"] {
                                            if tool.get(key).is_some() {
                                                tools[n][key] = tool[key].clone();
                                            }
                                        }
                                        for key in ["name", "arguments"] {
                                            append(
                                                &mut tools[n]["function"][key],
                                                &tool["function"][key],
                                            );
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
            }
            Protocol::Anthropic => match v["type"].as_str() {
                Some("message_start") => self.output = v["message"].clone(),
                Some("content_block_start") => {
                    let index = v["index"]
                        .as_u64()
                        .ok_or_else(|| upstream("invalid_stream_index"))?
                        as usize;
                    let blocks = self.output["content"]
                        .as_array_mut()
                        .ok_or_else(|| upstream("invalid_stream_event"))?;
                    if index != blocks.len() {
                        return Err(upstream("invalid_stream_index"));
                    }
                    blocks.push(v["content_block"].clone());
                }
                Some("content_block_delta") => {
                    let index = v["index"]
                        .as_u64()
                        .ok_or_else(|| upstream("invalid_stream_index"))?
                        as usize;
                    let block = self.output["content"]
                        .get_mut(index)
                        .ok_or_else(|| upstream("invalid_stream_index"))?;
                    match v["delta"]["type"].as_str() {
                        Some("text_delta") => append(&mut block["text"], &v["delta"]["text"]),
                        Some("thinking_delta") => {
                            append(&mut block["thinking"], &v["delta"]["thinking"])
                        }
                        Some("signature_delta") => {
                            append(&mut block["signature"], &v["delta"]["signature"])
                        }
                        Some("input_json_delta") => {
                            self.tool_json.entry(index).or_default().push_str(
                                v["delta"]["partial_json"]
                                    .as_str()
                                    .ok_or_else(|| upstream("invalid_tool_delta"))?,
                            )
                        }
                        _ => (),
                    }
                }
                Some("content_block_stop") => {
                    let index = v["index"]
                        .as_u64()
                        .ok_or_else(|| upstream("invalid_stream_index"))?
                        as usize;
                    if let Some(args) = self.tool_json.remove(&index) {
                        self.output["content"][index]["input"] = serde_json::from_str(&args)
                            .map_err(|_| upstream("invalid_tool_arguments"))?;
                    }
                }
                Some("message_delta") => {
                    if let Some(delta) = v["delta"].as_object() {
                        for (key, value) in delta {
                            self.output[key] = value.clone();
                        }
                    }
                    if let Some(usage) = v["usage"].as_object() {
                        for (key, value) in usage {
                            self.output["usage"][key] = value.clone();
                        }
                    }
                }
                Some("message_stop") => self.done = true,
                _ => return Ok(false),
            },
            Protocol::Gemini => {
                for key in [
                    "responseId",
                    "modelVersion",
                    "usageMetadata",
                    "promptFeedback",
                ] {
                    if v.get(key).is_some() {
                        self.output[key] = v[key].clone();
                    }
                }
                if let Some(candidate) = v["candidates"].get(0) {
                    if let Some(parts) = candidate["content"]["parts"].as_array() {
                        self.output["candidates"][0]["content"]["parts"]
                            .as_array_mut()
                            .unwrap()
                            .extend(parts.iter().cloned());
                    }
                    if candidate.get("finishReason").is_some_and(|v| !v.is_null()) {
                        self.output["candidates"][0]["finishReason"] =
                            candidate["finishReason"].clone();
                        self.done = true;
                    }
                }
            }
        }
        Ok(true)
    }
}
fn append(target: &mut Value, delta: &Value) {
    if let Some(delta) = delta.as_str() {
        *target = json!(format!("{}{}", target.as_str().unwrap_or(""), delta));
    }
}

pub async fn read(
    mut response: Response,
    protocol: Protocol,
    streaming: bool,
    emit: &mut dyn FnMut(Value) -> Result<(), Failure>,
) -> Result<Value, Failure> {
    let mut pending = Vec::new();
    let mut size = 0usize;
    let mut stream = Stream::new(protocol);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| upstream("provider_stream_interrupted"))?
    {
        size += chunk.len();
        if size > 8_388_608 {
            return Err(upstream("provider_response_too_large"));
        }
        pending.extend_from_slice(&chunk);
        if !streaming {
            continue;
        }
        loop {
            // SSE frames can cross arbitrary network and UTF-8 boundaries.
            let delimiter = pending
                .windows(2)
                .position(|w| w == b"\n\n")
                .map(|n| (n, 2))
                .into_iter()
                .chain(
                    pending
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map(|n| (n, 4)),
                )
                .min_by_key(|(position, _)| *position);
            let Some((end, length)) = delimiter else {
                break;
            };
            let frame = String::from_utf8(pending.drain(..end + length).collect())
                .map_err(|_| upstream("invalid_stream_encoding"))?;
            let data = frame
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("data:")
                        .map(|v| v.strip_prefix(' ').unwrap_or(v))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() && stream.event(&data)? {
                // Empty usage-only start frames are not content snapshots.
                if let Ok(snapshot) = stream.snapshot() {
                    emit(snapshot)?;
                }
            }
        }
    }
    if streaming {
        if !stream.done {
            return Err(upstream("provider_stream_incomplete"));
        }
        stream.snapshot()
    } else {
        let native =
            serde_json::from_slice(&pending).map_err(|_| upstream("invalid_provider_response"))?;
        normalized(native, protocol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_requests_keep_advanced_features_and_enforce_envelope_controls() {
        for protocol in [Protocol::Openai, Protocol::Anthropic, Protocol::Gemini] {
            let provider = Provider {
                protocol,
                base_url: "https://example.com/v1".into(),
                model: "configured-model".into(),
                api_key_env: "KEY".into(),
                input_per_million: 0.0,
                output_per_million: 0.0,
            };
            let input = json!({"model":"alias","stream":true,"max_tokens":12,"native_request":{"model":"wrong-model","stream":false,"max_tokens":9999,"messages":[{"role":"user","content":"Hello"}],"tools":[{"name":"weather"}],"generationConfig":{"maxOutputTokens":9999}}});
            let request = request(&Client::new(), &provider, &input, "local-key")
                .unwrap()
                .build()
                .unwrap();
            let body: Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(body["tools"][0]["name"], "weather");
            if protocol == Protocol::Gemini {
                assert_eq!(body["generationConfig"]["maxOutputTokens"], 12);
                assert!(
                    request
                        .url()
                        .path()
                        .ends_with("configured-model:streamGenerateContent")
                );
                assert_eq!(request.url().query(), Some("alt=sse"));
            } else {
                assert_eq!(body["max_tokens"], 12);
                assert_eq!(body["model"], "configured-model");
                assert_eq!(body["stream"], true);
            }
        }
    }
    #[test]
    fn openai_tool_deltas_are_accumulated() {
        let mut stream = Stream::new(Protocol::Openai);
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
    fn anthropic_tool_json_and_thinking_signatures_are_preserved() {
        let mut stream = Stream::new(Protocol::Anthropic);
        for event in [
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_1","name":"weather","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"city\":\"Toronto\"}"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"preserve-me"}}),
        ] {
            stream.event(&event.to_string()).unwrap();
        }
        let output = stream.snapshot().unwrap();
        assert_eq!(
            output["native_response"]["content"][1]["signature"],
            "preserve-me"
        );
        assert_eq!(
            output["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"Toronto\"}"
        );
    }
}
