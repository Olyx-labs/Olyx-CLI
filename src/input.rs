//! Bounded request loading shared by file and standard-input invocations.
use crate::error::CliError;
use serde_json::Value;
use std::io::Read;

pub(crate) fn read_request(path: &str, limit: usize) -> Result<Value, CliError> {
    if path == "-" {
        parse_request(std::io::stdin().lock(), limit)
    } else {
        let file = std::fs::File::open(path).map_err(|e| CliError::message(e.to_string()))?;
        parse_request(file, limit)
    }
}

fn parse_request(reader: impl Read, limit: usize) -> Result<Value, CliError> {
    let mut raw = String::new();
    reader
        .take((limit as u64).saturating_add(1))
        .read_to_string(&mut raw)
        .map_err(|e| CliError::message(e.to_string()))?;
    if raw.len() > limit {
        return Err(CliError::message("input_too_large"));
    }
    let body: Value = serde_json::from_str(&raw)
        .map_err(|_| CliError::message("input must be a JSON chat request"))?;
    if !body.is_object() {
        return Err(CliError::message("input must be a JSON object"));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_exact_byte_limit_and_preserves_request_fields() {
        let raw = r#"{"model":"fast","messages":[],"stream":true}"#;
        let body = parse_request(raw.as_bytes(), raw.len()).unwrap();
        assert_eq!(body["model"], "fast");
        assert_eq!(body["stream"], true);
        assert_eq!(
            parse_request(raw.as_bytes(), raw.len() - 1)
                .unwrap_err()
                .to_string(),
            "input_too_large"
        );
    }
    #[test]
    fn rejects_invalid_json_and_non_object_requests() {
        for raw in ["[]", "null", "42", "invalid"] {
            assert!(parse_request(raw.as_bytes(), 100).is_err());
        }
    }
    #[test]
    fn byte_limit_applies_to_multibyte_text() {
        let raw = r#"{"prompt":"é"}"#;
        assert!(parse_request(raw.as_bytes(), raw.len()).is_ok());
        assert!(parse_request(raw.as_bytes(), raw.chars().count()).is_err());
    }
}
