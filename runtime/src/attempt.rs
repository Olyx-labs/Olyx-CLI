//! Shared bookkeeping for one provider attempt; endpoint-specific decoding stays with callers.
use crate::{Failure, State, config::Provider, diagnostics, storage_error};
use reqwest::{RequestBuilder, Response};
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) struct Attempt<'a> {
    state: &'a State,
    trace_id: &'a str,
    candidate: &'a Provider,
    alias: &'a str,
    input: &'a Value,
}
impl<'a> Attempt<'a> {
    pub(crate) fn new(
        state: &'a State,
        trace_id: &'a str,
        candidate: &'a Provider,
        alias: &'a str,
        input: &'a Value,
    ) -> Self {
        Self {
            state,
            trace_id,
            candidate,
            alias,
            input,
        }
    }
    pub(crate) fn optional_credential(&self) -> Option<&str> {
        self.state
            .provider_keys
            .get(&self.candidate.api_key_env)
            .map(String::as_str)
    }
    pub(crate) fn credential(&self) -> Result<&str, Failure> {
        match self.optional_credential() {
            Some(key) => Ok(key),
            None if self.candidate.api_key_env.is_empty() => Ok(""),
            None => Err(diagnostics::missing_credential(self.candidate, self.alias)),
        }
    }
    pub(crate) async fn record(
        &self,
        output: &Value,
        metrics: (f64, u128, bool),
    ) -> Result<(), Failure> {
        self.state
            .store
            .lock()
            .await
            .record(
                self.trace_id,
                &self.candidate.model,
                self.input,
                output,
                metrics,
            )
            .map_err(storage_error)
    }
    pub(crate) async fn send(
        &self,
        request: RequestBuilder,
        started: Instant,
    ) -> Result<Response, Failure> {
        let status = match request.send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => Some(response.status().as_u16()),
            Err(_) => None,
        };
        let output = match status {
            Some(status) => {
                json!({"reason":"provider_rejected_request","status":status,"provider":self.candidate.base_url})
            }
            None => {
                json!({"reason":"provider_transport_failure","provider":self.candidate.base_url})
            }
        };
        self.record(&output, (0.0, started.elapsed().as_millis(), false))
            .await?;
        Err(diagnostics::provider_failure(
            self.candidate,
            self.alias,
            status,
        ))
    }
}
