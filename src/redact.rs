use serde_json::Value;

pub(crate) const REDACTED: &str = "[REDACTED]";

pub(crate) fn is_sensitive_key(key: &str) -> bool {
    // Normalize to lowercase with non-alphanumeric chars collapsed to '_' so
    // both snake_case and camelCase variants are caught
    // (e.g. "api_key" and "apiKey" both normalize to "apikey").
    let normalized = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    matches!(
        normalized.as_str(),
        "api_key"
            | "apikey"
            | "access_token"
            | "accesstoken"
            | "refresh_token"
            | "refreshtoken"
            | "token"
            | "secret"
            | "client_secret"
            | "clientsecret"
            | "authorization"
            | "password"
            | "openai_api_key"
            | "openaiapikey"
            | "anthropic_api_key"
            | "anthropicapikey"
            | "gemini_api_key"
            | "geminiapikey"
            | "private_key"
            | "privatekey"
            | "credentials"
            | "credential"
            | "bearer_token"
            | "bearertoken"
            | "x_api_key"
            | "xapikey"
    )
}

// Prevents stack overflow from adversarially nested JSON (e.g. in model responses or
// saved trace inputs). Real payloads are shallow; this cap is generous.
const MAX_SANITIZE_DEPTH: usize = 32;

pub(crate) fn sanitize_json_for_output(value: &Value) -> Value {
    sanitize_json_depth(value, 0)
}

fn sanitize_json_depth(value: &Value, depth: usize) -> Value {
    // Redact recursively because responses, trace bodies, and dry-run payloads
    // can carry secrets several levels deep in nested objects or arrays.
    if depth >= MAX_SANITIZE_DEPTH {
        return Value::String(REDACTED.to_string());
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let sanitized = if is_sensitive_key(key) {
                        Value::String(REDACTED.to_string())
                    } else {
                        sanitize_json_depth(value, depth + 1)
                    };
                    (key.clone(), sanitized)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| sanitize_json_depth(v, depth + 1))
                .collect(),
        ),
        Value::String(text) => Value::String(sanitize_text_for_output(text)),
        other => other.clone(),
    }
}

pub(crate) fn sanitize_text_for_output(raw: &str) -> String {
    // Broad token matching keeps the CLI safe in public logs without tying the
    // redaction logic to one provider or one secret format.
    let mut value = raw.to_string();
    value = redact_bearer_token(&value);

    for (prefix, min_suffix_len) in [
        ("ak_", 6usize),
        ("sk-", 10usize),
        ("ghp_", 12usize),
        ("ghs_", 12usize),
        ("gho_", 12usize),
        ("glpat-", 12usize),
        ("xoxb-", 8usize),
        ("xoxp-", 8usize),
        ("xoxs-", 8usize),
        ("xoxa-", 8usize),
        ("npm_", 10usize),
        ("SG.", 10usize),
        ("AKIA", 12usize),
        ("ASIA", 12usize),
    ] {
        value = redact_prefixed_token(&value, prefix, min_suffix_len);
    }

    value
}

fn redact_bearer_token(raw: &str) -> String {
    // Lowercase a copy for position-finding so "Bearer ", "bearer ", and "BEARER "
    // are all caught; slices are taken from the original string to preserve casing.
    let lower = raw.to_ascii_lowercase();
    let needle = "bearer ";
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0;

    while let Some(relative) = lower[cursor..].find(needle) {
        let start = cursor + relative;
        let token_start = start + needle.len();
        output.push_str(&raw[cursor..token_start]);

        let suffix_len = token_suffix_len(&raw[token_start..]);
        if suffix_len >= 6 {
            output.push_str(REDACTED);
            cursor = token_start + suffix_len;
        } else {
            cursor = token_start;
        }
    }

    output.push_str(&raw[cursor..]);
    output
}

fn redact_prefixed_token(raw: &str, prefix: &str, min_suffix_len: usize) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0;

    while let Some(relative) = raw[cursor..].find(prefix) {
        let start = cursor + relative;
        let suffix_start = start + prefix.len();
        output.push_str(&raw[cursor..start]);

        let suffix_len = token_suffix_len(&raw[suffix_start..]);
        if suffix_len >= min_suffix_len {
            output.push_str(prefix);
            output.push_str(REDACTED);
            cursor = suffix_start + suffix_len;
        } else {
            output.push_str(prefix);
            cursor = suffix_start;
        }
    }

    output.push_str(&raw[cursor..]);
    output
}

fn token_suffix_len(raw: &str) -> usize {
    // Accepts alphanumerics plus _ - . to cover JWTs (dots between header/payload/sig),
    // Slack tokens (dashes between segments), and underscored identifiers.
    // Stops at whitespace and shell metacharacters.
    raw.chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        .map(char::len_utf8)
        .sum()
}
