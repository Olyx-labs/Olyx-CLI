use crate::{Failure, config::Provider};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub message: String,
    pub hint: String,
    pub provider: Option<String>,
    pub alias: Option<String>,
    pub upstream_status: Option<u16>,
    pub retryable: bool,
    pub fallback: String,
}

pub fn provider_failure(candidate: &Provider, alias: &str, status: Option<u16>) -> Failure {
    let (code, message, hint, retryable) = match status {
        Some(401 | 403) => (
            "provider_authentication_failed",
            "The provider rejected the configured credentials or permissions.",
            "Check the configured provider credential and permissions. Running gateways load credentials at startup; restart after updating them.",
            false,
        ),
        Some(429) => (
            "provider_rate_limited",
            "The provider rate-limited this request.",
            "Wait before retrying, check provider quota, or configure an alternative provider.",
            true,
        ),
        Some(500..=599) => (
            "provider_unavailable",
            "The provider returned a server error.",
            "Retry later or configure another fallback candidate.",
            true,
        ),
        Some(_) => (
            "provider_request_rejected",
            "The provider rejected the request.",
            "Check the configured model, endpoint, and request parameters. Inspect the trace for the attempted route.",
            false,
        ),
        None => (
            "provider_transport_failure",
            "The provider could not be reached.",
            "Check network access, DNS, TLS, and the configured endpoint.",
            true,
        ),
    };
    Failure {
        status: 502,
        code: code.into(),
        trace_id: None,
        diagnostic: Some(Box::new(Diagnostic {
            message: message.into(),
            hint: hint.into(),
            provider: reqwest::Url::parse(&candidate.base_url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned)),
            alias: Some(alias.into()),
            upstream_status: status,
            retryable,
            fallback: if retryable {
                "exhausted"
            } else {
                "stopped_non_retryable"
            }
            .into(),
        })),
    }
}

impl Failure {
    pub fn new(status: u16, code: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            trace_id: None,
            diagnostic: None,
        }
    }
    pub fn with_trace(mut self, id: impl Into<String>) -> Self {
        self.trace_id = Some(id.into());
        self
    }
    pub fn with_trace_ref(&self, id: &str) -> Value {
        let mut value = self.payload()["error"].clone();
        value["trace_id"] = id.into();
        value
    }
    pub fn payload(&self) -> Value {
        let mut error = serde_json::json!({"code":self.code,"message":self.code,"type":if self.status < 500 {"invalid_request_error"} else {"server_error"},"param":null,"trace_id":self.trace_id,"retryable":false});
        if let Some(d) = &self.diagnostic {
            error["message"] = d.message.clone().into();
            error["hint"] = d.hint.clone().into();
            error["provider"] = d.provider.clone().into();
            error["alias"] = d.alias.clone().into();
            error["upstream_status"] = d.upstream_status.into();
            error["retryable"] = d.retryable.into();
            error["fallback"] = d.fallback.clone().into();
        }
        serde_json::json!({"error":error})
    }
}

pub fn missing_credential(candidate: &Provider, alias: &str) -> Failure {
    let mut error = provider_failure(candidate, alias, None);
    error.status = 503;
    error.code = "provider_credentials_unavailable".into();
    let d = error.diagnostic.as_mut().unwrap();
    d.message = "The configured credential environment variable is missing or empty.".into();
    d.hint = format!(
        "Export {} and restart a running gateway; then run oy doctor with the same configuration.",
        candidate.api_key_env
    );
    d.retryable = false;
    d.fallback = "stopped_missing_credentials".into();
    error
}

pub fn response_failure(mut error: Failure, candidate: &Provider, alias: &str) -> Failure {
    let mut context = provider_failure(candidate, alias, None).diagnostic.unwrap();
    context.message =
        "The provider response was incomplete, invalid, or could not be delivered.".into();
    context.hint = "Inspect this trace and the provider response contract. No fallback was attempted after a successful upstream response; a new request may incur additional usage.".into();
    context.retryable = false;
    context.fallback = "stopped_response_started".into();
    error.diagnostic = Some(context);
    error
}
