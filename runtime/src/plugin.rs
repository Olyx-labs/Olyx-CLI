//! Version 1 subprocess provider contract. Installed adapters are trusted code.
use crate::{Failure, config::Provider};
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

fn error(code: &str) -> Failure {
    Failure::new(502, code)
}
fn required(body: &Value, operation: &str) -> Vec<&'static str> {
    let mut capabilities = vec![if operation == "embeddings" {
        "embeddings"
    } else {
        "chat"
    }];
    if body["stream"] == true {
        capabilities.push("streaming");
    }
    if body["tools"].as_array().is_some_and(|t| !t.is_empty()) || body.get("tool_choice").is_some()
    {
        capabilities.push("tools");
    }
    capabilities
}

pub async fn execute(
    provider: &Provider,
    operation: &str,
    body: &Value,
    key: Option<&str>,
    limit: usize,
    emit: &mut (dyn FnMut(Value) -> Result<(), Failure> + Send),
) -> Result<Value, Failure> {
    let plugin = provider
        .plugin
        .as_ref()
        .ok_or_else(|| error("plugin_configuration_required"))?;
    let needed = required(body, operation);
    if needed
        .iter()
        .any(|c| !plugin.capabilities.iter().any(|v| v == c))
    {
        return Err(Failure::new(400, "plugin_capability_unsupported"));
    }
    if !provider.api_key_env.is_empty() && key.is_none() {
        return Err(error("missing_provider_credential"));
    }
    let task = async {
        let mut command = Command::new(&plugin.command);
        command
            .args(&plugin.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for name in ["PATH", "SYSTEMROOT", "SystemRoot", "TEMP", "TMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        for name in &plugin.env {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if let Some(key) = key {
            command.env(&provider.api_key_env, key);
        }
        let mut child = command.spawn().map_err(|_| error("plugin_start_failed"))?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| error("plugin_io_failed"))?;
        let message = json!({"protocol_version":1,"operation":operation,"model":provider.model,"request":body});
        input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|_| error("plugin_io_failed"))?;
        drop(input);
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| error("plugin_io_failed"))?;
        let mut pending = Vec::new();
        let mut bytes = [0u8; 4096];
        let mut total = 0usize;
        let mut hello = false;
        let mut result = None;
        let mut last_snapshot = None;
        loop {
            let count = stdout
                .read(&mut bytes)
                .await
                .map_err(|_| error("plugin_io_failed"))?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count)
                .ok_or_else(|| error("plugin_response_too_large"))?;
            if total > limit {
                return Err(error("plugin_response_too_large"));
            }
            pending.extend_from_slice(&bytes[..count]);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                if end > 1_048_576 {
                    return Err(error("plugin_event_too_large"));
                }
                let line: Vec<u8> = pending.drain(..=end).collect();
                let event: Value =
                    serde_json::from_slice(&line).map_err(|_| error("plugin_invalid_event"))?;
                if result.is_some() {
                    return Err(error("plugin_event_after_result"));
                }
                if !hello {
                    if event["type"] != "hello" || event["protocol_version"] != 1 {
                        return Err(error("plugin_version_mismatch"));
                    }
                    let capabilities = event["capabilities"]
                        .as_array()
                        .ok_or_else(|| error("plugin_invalid_capabilities"))?;
                    if needed
                        .iter()
                        .any(|c| !capabilities.iter().any(|v| v.as_str() == Some(c)))
                    {
                        return Err(Failure::new(400, "plugin_capability_unsupported"));
                    }
                    hello = true;
                    continue;
                }
                match event["type"].as_str() {
                    Some("snapshot") if operation == "chat" && body["stream"] == true => {
                        let output = crate::response::checked(event["output"].clone(), false)?;
                        emit(output.clone())?;
                        last_snapshot = Some(output);
                    }
                    Some("result") => {
                        let output = event
                            .get("output")
                            .ok_or_else(|| error("plugin_missing_output"))?
                            .clone();
                        if operation == "chat"
                            && output["choices"].as_array().is_none_or(|choices| {
                                choices
                                    .iter()
                                    .any(|c| c["finish_reason"].as_str().is_none_or(str::is_empty))
                            })
                        {
                            return Err(error("plugin_result_incomplete"));
                        }
                        let output = if operation == "chat" {
                            crate::response::checked(output, true)?
                        } else {
                            output
                        };
                        if body["stream"] == true && last_snapshot.as_ref() != Some(&output) {
                            return Err(error("plugin_stream_result_mismatch"));
                        }
                        result = Some(output);
                    }
                    Some("error") => {
                        return Err(match event["code"].as_str() {
                            Some("unavailable") => Failure::new(503, "plugin_unavailable"),
                            Some("rate_limited") => Failure::new(429, "plugin_rate_limited"),
                            Some("authentication_failed") => {
                                Failure::new(502, "plugin_authentication_failed")
                            }
                            Some("invalid_request") => Failure::new(400, "plugin_request_rejected"),
                            _ => error("plugin_failed"),
                        });
                    }
                    _ => return Err(error("plugin_invalid_event")),
                }
            }
            if pending.len() > 1_048_576 {
                return Err(error("plugin_event_too_large"));
            }
        }
        if !pending.is_empty() {
            return Err(error("plugin_unterminated_event"));
        }
        if !child
            .wait()
            .await
            .map_err(|_| error("plugin_io_failed"))?
            .success()
        {
            return Err(error("plugin_exit_failed"));
        }
        result.ok_or_else(|| error("plugin_result_missing"))
    };
    tokio::time::timeout(std::time::Duration::from_millis(plugin.timeout_ms), task)
        .await
        .map_err(|_| error("plugin_timeout"))?
}
