//! Validate provider envelopes without interpreting generated text or tool JSON.
use crate::Failure;
use serde_json::Value;
fn invalid() -> Failure {
    Failure::new(502, "invalid_provider_response")
}

pub(crate) fn filtered(reason: &Value) -> bool {
    matches!(
        reason.as_str().unwrap_or("").to_ascii_lowercase().as_str(),
        "content_filter"
            | "guardrail_intervened"
            | "safety"
            | "recitation"
            | "blocklist"
            | "prohibited_content"
            | "spii"
            | "image_safety"
    )
}

pub(crate) fn usage(value: &Value) -> Result<(), Failure> {
    let invalid = || Failure::new(502, "invalid_provider_usage");
    if value.is_null() {
        return Ok(());
    }
    let object = value.as_object().ok_or_else(invalid)?;
    for key in [
        "prompt_tokens",
        "completion_tokens",
        "total_tokens",
        "cached_tokens",
        "input_tokens",
        "output_tokens",
        "inputTokens",
        "outputTokens",
        "promptTokenCount",
        "candidatesTokenCount",
        "thoughtsTokenCount",
        "toolUsePromptTokenCount",
        "totalTokenCount",
        "cachedContentTokenCount",
    ] {
        if object
            .get(key)
            .is_some_and(|v| !v.is_null() && v.as_u64().is_none())
        {
            return Err(invalid());
        }
    }
    for (input, output) in [
        ("prompt_tokens", "completion_tokens"),
        ("input_tokens", "output_tokens"),
        ("inputTokens", "outputTokens"),
    ] {
        if let (Some(input), Some(output)) = (value[input].as_u64(), value[output].as_u64()) {
            input.checked_add(output).ok_or_else(invalid)?;
        }
    }
    Ok(())
}

pub(crate) fn checked(output: Value, complete: bool) -> Result<Value, Failure> {
    if !output.is_object() || output.get("error").is_some_and(|v| !v.is_null()) {
        return Err(invalid());
    }
    usage(&output["usage"])?;
    usage(&output["native_response"]["usage"])?;
    usage(&output["native_response"]["usage"]["tokens"])?;
    usage(&output["native_response"]["usageMetadata"])?;
    let choices = output["choices"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(invalid)?;
    let mut indices = std::collections::BTreeSet::new();
    for (position, choice) in choices.iter().enumerate() {
        let index = match choice.get("index") {
            Some(index) => index.as_u64().ok_or_else(invalid)?,
            None => position as u64,
        };
        if !indices.insert(index) {
            return Err(invalid());
        }
        let message = &choice["message"];
        if !choice.is_object()
            || !message.is_object()
            || message.get("role").is_some_and(|v| v != "assistant")
        {
            return Err(invalid());
        }
        if let Some(content) = message.get("content") {
            if let Some(parts) = content.as_array() {
                if parts.iter().any(|part| {
                    !part.is_object()
                        || !part["type"].is_string()
                        || part.get("text").is_some_and(|v| !v.is_string())
                }) {
                    return Err(invalid());
                }
            } else if !content.is_null() && !content.is_string() {
                return Err(invalid());
            }
        }
        if message
            .get("refusal")
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(invalid());
        }
        if let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
            for call in calls.as_array().ok_or_else(invalid)? {
                if !call.is_object() {
                    return Err(invalid());
                }
                if call.get("id").is_some_and(|v| !v.is_string())
                    || (complete && call["id"].as_str().is_none_or(str::is_empty))
                {
                    return Err(invalid());
                }
                let custom = call["type"] == "custom";
                if call
                    .get("type")
                    .is_some_and(|v| v != "function" && v != "custom")
                {
                    return Err(invalid());
                }
                let function = &call[if custom { "custom" } else { "function" }];
                if !function.is_object() {
                    return Err(invalid());
                }
                for key in ["name", if custom { "input" } else { "arguments" }] {
                    if function.get(key).is_some_and(|v| !v.is_string())
                        || (complete && function.get(key).is_none())
                    {
                        return Err(invalid());
                    }
                }
                if complete && function["name"].as_str().is_none_or(str::is_empty) {
                    return Err(invalid());
                }
            }
        }
        if message
            .get("function_call")
            .filter(|v| !v.is_null())
            .is_some_and(|call| {
                !call.is_object() || !call["name"].is_string() || !call["arguments"].is_string()
            })
        {
            return Err(invalid());
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn duplicate_and_invalid_choice_indices_are_rejected() {
        for indices in [
            [json!(0), json!(0)],
            [json!(-1), json!(1)],
            [json!(0), Value::Null],
        ] {
            let output = json!({"choices":[{"index":indices[0],"message":{"content":"one"}},{"index":indices[1],"message":{"content":"two"}}]});
            assert!(checked(output, true).is_err());
        }
    }

    #[test]
    fn usage_rejects_negative_fractional_scalar_and_overflow_counters() {
        for value in [
            json!(-1),
            json!("bad"),
            json!({"prompt_tokens":-1}),
            json!({"total_tokens":1.5}),
            json!({"prompt_tokens":u64::MAX,"completion_tokens":1}),
        ] {
            assert_eq!(usage(&value).unwrap_err().code, "invalid_provider_usage");
        }
        assert!(usage(&json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":9})).is_ok());
        assert!(usage(&Value::Null).is_ok());
    }

    #[test]
    fn malformed_tools_roles_content_and_errors_are_rejected() {
        for message in [
            json!({"role":"user","content":"bad"}),
            json!({"content":[42]}),
            json!({"tool_calls":[null]}),
            json!({"tool_calls":[{"id":"one","function":{"name":"lookup","arguments":{}}}]}),
            json!({"tool_calls":[{"id":"one","function":{"arguments":"{}"}}]}),
        ] {
            assert!(checked(json!({"choices":[{"message":message}]}), true).is_err());
        }
        assert!(
            checked(
                json!({"error":{"message":"bad"},"choices":[{"message":{"content":"ok"}}]}),
                true
            )
            .is_err()
        );
    }
    #[test]
    fn partial_tool_fields_and_truncated_json_are_not_confused() {
        let partial = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"","arguments":""}}]}}]});
        assert!(checked(partial.clone(), false).is_ok());
        assert!(checked(partial, true).is_err());
        assert!(checked(json!({"choices":[{"finish_reason":"length","message":{"tool_calls":[{"id":"one","type":"function","function":{"name":"lookup","arguments":"{\\\"q\\\":"}}]}}]}),true).is_ok());
    }
}
