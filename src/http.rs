use serde_json::{json, Value};

use crate::error::CliError;
use crate::redact::{sanitize_json_for_output, sanitize_text_for_output};

pub(crate) fn api_key_present(api_key: Option<&str>) -> bool {
    api_key.is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn normalize_base_url(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") {
        let host = extract_host(trimmed.trim_start_matches("https://"));
        if is_ssrf_blocked_host(host) {
            return Err(CliError::usage(
                "base URL resolves to a blocked address (cloud metadata or link-local range); use the public Olyx Cloud endpoint or a private-deployment hostname",
            ));
        }
        Ok(trimmed.to_string())
    } else if trimmed.starts_with("http://") {
        // Allow http only for loopback addresses (local development and CI smoke checks).
        let host = extract_host(trimmed.trim_start_matches("http://"));
        if matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
            Ok(trimmed.to_string())
        } else {
            Err(CliError::usage(
                "http:// is only permitted for localhost; use https:// for remote endpoints to prevent bearer token exposure",
            ))
        }
    } else {
        Err(CliError::usage(
            "base URL must start with https:// (or http://localhost for local development)",
        ))
    }
}

fn extract_host(authority: &str) -> &str {
    // Strip path, then strip port. Handles IPv6 literals ([::1]) by leaving them intact.
    let without_path = authority.split('/').next().unwrap_or("");
    if without_path.starts_with('[') {
        // IPv6 literal — return through closing bracket.
        without_path
            .find(']')
            .map(|i| &without_path[..=i])
            .unwrap_or(without_path)
    } else {
        without_path.split(':').next().unwrap_or("")
    }
}

fn is_ssrf_blocked_host(host: &str) -> bool {
    // Block cloud instance metadata endpoints and link-local ranges.
    // These addresses are only reachable from inside a cloud VM, so a
    // misconfigured OLYX_BASE_URL pointing at them would exfiltrate the
    // bearer token to the metadata service rather than Olyx Cloud.
    // 169.254.x.x covers AWS IMDSv1/v2, Azure IMDS, and the full RFC 3927
    // link-local block. Named metadata hostnames are checked explicitly.
    matches!(
        host,
        "169.254.169.254"
            | "metadata.google.internal"
            | "100.100.100.200"
            | "169.254.0.2"
            | "169.254.0.1"
            | "metadata.internal"
    ) || host.starts_with("169.254.")
        || host.starts_with("fd00:ec2:")
}

pub(crate) async fn request_json(
    method: &str,
    base_url: &str,
    path: &str,
    api_key: Option<&str>,
    body: Option<&Value>,
    dry_run: bool,
) -> Result<Value, CliError> {
    // Dry-run is a public-facing inspection path, so it returns the exact
    // request shape while redacting secrets instead of inventing mock payloads.
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "method": method,
            "url": format!("{base_url}{path}"),
            "api_key_present": api_key_present(api_key),
            "body": body.map(sanitize_json_for_output)
        }));
    }

    let url = format!("{base_url}{path}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| CliError::message(format!("http client error: {e}")))?;
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| CliError::message(format!("invalid HTTP method: {e}")))?;
    let mut request = client
        .request(method, &url)
        .header(reqwest::header::ACCEPT, "application/json");

    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    if let Some(body) = body {
        request = request.json(body);
    }

    let response = request
        .send()
        .await
        .map_err(|e| CliError::message(format!("request failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| CliError::message(format!("response read failed: {e}")))?;
    let json_body: Value =
        serde_json::from_str(text.trim()).unwrap_or_else(|_| json!({ "raw": text.trim() }));

    if status.is_success() {
        Ok(json_body)
    } else {
        Err(CliError::message(format!(
            "request returned HTTP {status}: {}",
            format_api_error(&json_body)
        )))
    }
}

fn format_api_error(value: &Value) -> String {
    let message = value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .map(sanitize_text_for_output);
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .map(sanitize_text_for_output);
    let request_id = value
        .get("request_id")
        .and_then(Value::as_str)
        .map(sanitize_text_for_output);

    match (message.as_deref(), code.as_deref(), request_id.as_deref()) {
        (Some(message), Some(code), Some(request_id)) => {
            format!("{message} ({code}, request_id={request_id})")
        }
        (Some(message), Some(code), None) => format!("{message} ({code})"),
        (Some(message), None, Some(request_id)) => format!("{message} (request_id={request_id})"),
        (Some(message), None, None) => message.to_string(),
        _ => truncate_for_error(&value.to_string()),
    }
}

fn truncate_for_error(raw: &str) -> String {
    let trimmed = sanitize_text_for_output(raw);
    let trimmed = trimmed.trim();
    if trimmed.len() > 500 {
        format!("{}...", trimmed.chars().take(500).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
