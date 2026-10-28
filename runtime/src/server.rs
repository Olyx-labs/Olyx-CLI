//! Application HTTP endpoint backed exclusively by the public local runtime.
use crate::{Failure, State as Runtime, config::Config};
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures_util::stream;
use serde_json::{Value, json};
use std::{collections::BTreeMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::{Semaphore, mpsc};

#[derive(Clone)]
struct App {
    runtime: Arc<Runtime>,
    models: Vec<String>,
    key: Option<String>,
    slots: Arc<Semaphore>,
}

pub async fn serve(
    config: Config,
    listen: SocketAddr,
    key: Option<String>,
    max_concurrency: usize,
) -> Result<()> {
    if !listen.ip().is_loopback() && key.as_ref().is_none_or(|key| key.trim().is_empty()) {
        bail!("OLYX_SERVER_API_KEY is required when binding a non-loopback address");
    }
    if key.as_ref().is_some_and(|key| {
        key.trim().is_empty() || key.contains(char::is_whitespace) || !key.is_ascii()
    }) {
        bail!("OLYX_SERVER_API_KEY must be a nonempty ASCII token without whitespace");
    }
    if max_concurrency == 0 {
        bail!("max concurrency must be positive");
    }
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .context("cannot bind server address")?;
    let actual = listener.local_addr()?;
    let router = router(config, key, max_concurrency)?;
    eprintln!("Olyx listening on http://{actual}; application base URL: http://{actual}/v1");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
        .context("server failed")
}
fn router(config: Config, key: Option<String>, max_concurrency: usize) -> Result<Router> {
    let aliases = config.models.keys().cloned().collect();
    let limit = config.policy.max_body_bytes;
    let state = App {
        runtime: Runtime::new(config)?,
        models: aliases,
        key,
        slots: Arc::new(Semaphore::new(max_concurrency)),
    };
    Ok(Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/embeddings", post(embeddings))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/readyz", get(ready))
        .layer(DefaultBodyLimit::max(limit))
        .layer(middleware::map_response(security_headers))
        .with_state(state))
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = term.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    eprintln!("Olyx shutting down; waiting for in-flight requests.");
}
fn error(status: StatusCode, code: &str) -> Response {
    (status,Json(json!({"error":{"message":code,"type":if status.is_client_error(){"invalid_request_error"}else{"server_error"},"code":code,"param":null}}))).into_response()
}
fn failure(error_value: Failure) -> Response {
    let mut response = (
        StatusCode::from_u16(error_value.status).unwrap_or(StatusCode::BAD_GATEWAY),
        Json(error_value.payload()),
    )
        .into_response();
    if let Some(id) = &error_value.trace_id {
        trace_header(&mut response, id);
    }
    response
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim_start_matches(' '))
}
fn authorized(app: &App, headers: &HeaderMap) -> bool {
    let Some(expected) = app.key.as_ref() else {
        return true;
    };
    let Some(supplied) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(bearer_token)
    else {
        return false;
    };
    supplied.len() == expected.len()
        && supplied
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}
async fn security_headers(mut response: Response) -> Response {
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

// Reject unauthorized callers before JSON extraction reads or buffers their body.
async fn authenticate(State(app): State<App>, request: Request, next: Next) -> Response {
    if !authorized(&app, request.headers()) {
        return error(StatusCode::UNAUTHORIZED, "invalid_server_api_key");
    }
    next.run(request).await
}
async fn ready(State(app): State<App>) -> Response {
    match app.runtime.healthy().await {
        Ok(()) => Json(json!({"status":"ready","checks":{"storage":"pass","provider_connectivity":"not_checked","provider_credentials":"not_validated"}})).into_response(),
        Err(e) => failure(e),
    }
}
async fn models(State(app): State<App>) -> Response {
    Json(json!({"object":"list","data":app.models.iter().map(|id|json!({"id":id,"object":"model","created":0,"owned_by":"oy"})).collect::<Vec<_>>()})).into_response()
}
fn trace_header(response: &mut Response, trace: &str) {
    if let Ok(value) = HeaderValue::from_str(trace) {
        response.headers_mut().insert("x-olyx-trace-id", value);
    }
}
fn finish_reason(value: &Value, tools: bool) -> Value {
    match value
        .as_str()
        .unwrap_or("stop")
        .to_ascii_lowercase()
        .as_str()
    {
        "max_tokens" | "max_output_tokens" | "length" => json!("length"),
        _ if crate::response::filtered(value) => json!("content_filter"),
        _ if tools => json!("tool_calls"),
        _ => json!("stop"),
    }
}
fn completion(mut output: Value, trace: &str, model: &str, created: i64) -> Value {
    output["id"] = json!(format!("chatcmpl-{trace}"));
    output["object"] = json!("chat.completion");
    output["created"] = json!(created);
    output["model"] = json!(model);
    if let Some(choices) = output["choices"].as_array_mut() {
        for (index, choice) in choices.iter_mut().enumerate() {
            if choice.get("index").is_none() {
                choice["index"] = json!(index);
            }
            choice["finish_reason"] = finish_reason(
                &choice["finish_reason"],
                choice["message"]["tool_calls"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty()),
            );
        }
    }
    output
}
async fn chat(State(app): State<App>, body: Result<Json<Value>, JsonRejection>) -> Response {
    let mut body = match body {
        Ok(Json(body)) => body,
        Err(rejection) => return error(rejection.status(), "invalid_json_request"),
    };
    if !body.is_object() {
        return error(StatusCode::BAD_REQUEST, "request_must_be_object");
    }
    let model = match body["model"].as_str() {
        Some(value) => value.to_owned(),
        None => return error(StatusCode::BAD_REQUEST, "model_required"),
    };
    if !app.models.contains(&model) {
        return error(StatusCode::BAD_REQUEST, "unknown_model");
    }
    let slot = match app.slots.clone().try_acquire_owned() {
        Ok(slot) => slot,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "server_busy"),
    };
    let created = chrono::Utc::now().timestamp();
    if body["stream"] != true {
        return match app.runtime.route(body).await {
            Ok(result) => {
                let trace = result["trace_id"].as_str().unwrap_or("");
                let mut response =
                    Json(completion(result["output"].clone(), trace, &model, created))
                        .into_response();
                trace_header(&mut response, trace);
                drop(slot);
                response
            }
            Err(e) => failure(e),
        };
    }
    let usage = body["stream_options"]["include_usage"] == true;
    body.as_object_mut().unwrap().remove("stream_options");
    let (sender, mut receiver) = mpsc::channel::<Message>(8);
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let mut delta = Delta::new(model, created);
        let mut trace = String::new();
        let mut emit = |event: Value| {
            trace = event["trace_id"].as_str().unwrap_or("").to_owned();
            for value in delta.snapshot(&event["output"], &trace, false) {
                tokio::task::block_in_place(|| {
                    sender.blocking_send(Message::Data(trace.clone(), value))
                })
                .map_err(|_| Failure::new(499, "client_disconnected"))?;
            }
            Ok(())
        };
        let result = handle.block_on(async {
            tokio::select! {
                result = app.runtime.route_with_events(body, &mut emit) => result,
                _ = sender.closed() => Err(Failure::new(499, "client_disconnected")),
            }
        });
        match result {
            Ok(result) => {
                trace = result["trace_id"].as_str().unwrap_or("").to_owned();
                for value in delta.snapshot(&result["output"], &trace, true) {
                    if tokio::task::block_in_place(|| {
                        sender.blocking_send(Message::Data(trace.clone(), value))
                    })
                    .is_err()
                    {
                        return;
                    }
                }
                if usage {
                    let response =
                        completion(result["output"].clone(), &trace, &delta.model, created);
                    let value = json!({"id":format!("chatcmpl-{trace}"),"object":"chat.completion.chunk","created":created,"model":delta.model,"choices":[],"usage":response["usage"]});
                    if tokio::task::block_in_place(|| {
                        sender.blocking_send(Message::Data(trace.clone(), value))
                    })
                    .is_err()
                    {
                        return;
                    }
                }
                let _ = sender.blocking_send(Message::Done(trace));
            }
            Err(e) => {
                let _ = sender.blocking_send(Message::Error(e));
            }
        }
    });
    let first = match receiver.recv().await {
        Some(Message::Error(e)) => return failure(e),
        Some(value) => value,
        None => return error(StatusCode::BAD_GATEWAY, "provider_stream_incomplete"),
    };
    let trace = match &first {
        Message::Data(trace, _) | Message::Done(trace) => trace.clone(),
        Message::Error(_) => unreachable!(),
    };
    let events = stream::unfold(
        (receiver, Some(first), false),
        |(mut receiver, mut first, closed)| async move {
            if closed {
                return None;
            }
            let message = match first.take() {
                Some(value) => value,
                None => receiver.recv().await?,
            };
            let (data, closed) = match message {
                Message::Data(_, value) => (value.to_string(), false),
                Message::Done(_) => ("[DONE]".into(), true),
                Message::Error(e) => (e.payload().to_string(), true),
            };
            Some((
                Ok::<_, Infallible>(Event::default().data(data)),
                (receiver, first, closed),
            ))
        },
    );
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response();
    trace_header(&mut response, &trace);
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}
enum Message {
    Data(String, Value),
    Done(String),
    Error(Failure),
}
struct Delta {
    model: String,
    created: i64,
    previous: BTreeMap<usize, Value>,
    finished: std::collections::BTreeSet<usize>,
}
impl Delta {
    fn new(model: String, created: i64) -> Self {
        Self {
            model,
            created,
            previous: BTreeMap::new(),
            finished: Default::default(),
        }
    }
    fn snapshot(&mut self, output: &Value, trace: &str, completed: bool) -> Vec<Value> {
        let mut events = Vec::new();
        for (fallback, choice) in output["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let i = choice["index"].as_u64().unwrap_or(fallback as u64) as usize;
            let message = &choice["message"];
            let old = self.previous.get(&i);
            let mut delta = json!({});
            if old.is_none() {
                delta["role"] = json!("assistant");
            }
            if let Some(object) = message.as_object() {
                for (key, value) in object {
                    if key == "role" {
                        continue;
                    }
                    if key == "tool_calls" {
                        let mut calls = Vec::new();
                        for (n, tool) in value.as_array().into_iter().flatten().enumerate() {
                            let prev = old.and_then(|v| v["tool_calls"].get(n));
                            let mut change = json!({"index":n});
                            for key in ["id", "type"] {
                                if prev.is_none_or(|p| p[key] != tool[key]) && !tool[key].is_null()
                                {
                                    change[key] = tool[key].clone();
                                }
                            }
                            for key in ["name", "arguments"] {
                                if let Some(text) = tool["function"][key].as_str() {
                                    let previous = prev
                                        .and_then(|p| p["function"][key].as_str())
                                        .unwrap_or("");
                                    if let Some(tail) =
                                        text.strip_prefix(previous).filter(|s| !s.is_empty())
                                    {
                                        if change.get("function").is_none() {
                                            change["function"] = json!({});
                                        }
                                        change["function"][key] = json!(tail);
                                    }
                                }
                            }
                            if change.as_object().unwrap().len() > 1 {
                                calls.push(change);
                            }
                        }
                        if !calls.is_empty() {
                            delta[key] = json!(calls);
                        }
                    } else if let Some(text) = value.as_str() {
                        let previous = old.and_then(|v| v[key].as_str()).unwrap_or("");
                        if let Some(tail) = text.strip_prefix(previous).filter(|s| !s.is_empty()) {
                            delta[key] = json!(tail);
                        }
                    } else if !value.is_null() && old.is_none_or(|p| p[key] != *value) {
                        delta[key] = value.clone();
                    }
                }
            }
            // Finish only after provider parsing succeeds; partial streams must not
            // advertise success merely because an upstream finish field arrived.
            let finish = if completed && !self.finished.contains(&i) {
                self.finished.insert(i);
                finish_reason(
                    &choice["finish_reason"],
                    message["tool_calls"]
                        .as_array()
                        .is_some_and(|v| !v.is_empty()),
                )
            } else {
                Value::Null
            };
            if !delta.as_object().unwrap().is_empty() || !finish.is_null() {
                events.push(json!({"id":format!("chatcmpl-{trace}"),"object":"chat.completion.chunk","created":self.created,"model":self.model,"choices":[{"index":i,"delta":delta,"finish_reason":finish}],"usage":null}));
            }
            self.previous.insert(i, message.clone());
        }
        events
    }
}

async fn embeddings(State(app): State<App>, body: Result<Json<Value>, JsonRejection>) -> Response {
    let body = match body {
        Ok(Json(value)) => value,
        Err(e) => return error(e.status(), "invalid_json_request"),
    };
    let _slot = match app.slots.clone().try_acquire_owned() {
        Ok(slot) => slot,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "server_busy"),
    };
    match app.runtime.embeddings(body).await {
        Ok(result) => {
            let mut response = Json(result["output"].clone()).into_response();
            trace_header(&mut response, result["trace_id"].as_str().unwrap_or(""));
            response
        }
        Err(e) => failure(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_scheme_is_case_insensitive_but_token_is_exact() {
        for value in ["Bearer Key", "bearer Key", "bEaReR Key", "BEARER   Key"] {
            assert_eq!(bearer_token(value), Some("Key"));
        }
        assert_eq!(bearer_token("Bearer key"), Some("key"));
        assert_eq!(bearer_token("Bearer Key "), Some("Key "));
        assert_eq!(bearer_token("Basic Key"), None);
        assert_eq!(bearer_token("Bearer\tKey"), None);
    }

    #[test]
    fn choice_indices_stay_attached_to_their_content() {
        let output = completion(
            json!({"choices":[{"index":1,"message":{"content":"one"}},{"index":0,"message":{"content":"zero"}}]}),
            "trace",
            "model",
            0,
        );
        assert_eq!(output["choices"][0]["index"], 1);
        assert_eq!(output["choices"][0]["message"]["content"], "one");
        assert_eq!(output["choices"][1]["index"], 0);
    }

    #[test]
    fn provider_filter_reasons_are_not_reported_as_successful_stops() {
        for reason in ["SAFETY", "RECITATION", "BLOCKLIST", "PROHIBITED_CONTENT"] {
            assert_eq!(finish_reason(&json!(reason), false), "content_filter");
            assert_eq!(finish_reason(&json!(reason), true), "content_filter");
        }
    }

    #[test]
    fn http_completion_preserves_reported_token_totals() {
        let output = json!({"choices":[],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":9}});
        assert_eq!(
            completion(output, "trace", "model", 0)["usage"]["total_tokens"],
            9
        );
    }

    #[test]
    fn truncation_is_preserved_for_json_and_streamed_tools() {
        let output = json!({"choices":[{"index":0,"message":{"tool_calls":[{"id":"one","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]},"finish_reason":"length"}]});
        assert_eq!(
            completion(output.clone(), "trace", "model", 0)["choices"][0]["finish_reason"],
            "length"
        );
        let mut delta = Delta::new("model".into(), 0);
        assert_eq!(
            delta.snapshot(&output, "trace", true)[0]["choices"][0]["finish_reason"],
            "length"
        );
    }

    #[test]
    fn snapshots_emit_only_new_text_and_tool_arguments() {
        let mut delta = Delta::new("fast".into(), 42);
        let first = json!({"choices":[{"message":{"content":"Hello","tool_calls":[{"id":"call1","type":"function","function":{"name":"weather","arguments":"{\"city\":"}}]},"finish_reason":null}]});
        let events = delta.snapshot(&first, "trace", false);
        assert_eq!(events[0]["choices"][0]["delta"]["content"], "Hello");
        assert!(delta.snapshot(&first, "trace", false).is_empty());
        let second = json!({"choices":[{"message":{"content":"Hello 🦀","tool_calls":[{"id":"call1","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Paris\"}"}}]},"finish_reason":"tool_calls"}]});
        let events = delta.snapshot(&second, "trace", false);
        assert_eq!(events[0]["choices"][0]["delta"]["content"], " 🦀");
        assert_eq!(
            events[0]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "\"Paris\"}"
        );
        assert!(events[0]["choices"][0]["finish_reason"].is_null());
        let final_events = delta.snapshot(&second, "trace", true);
        assert_eq!(final_events[0]["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(final_events[0]["id"], events[0]["id"]);
        assert_eq!(final_events[0]["created"], 42);
        assert!(delta.snapshot(&second, "trace", true).is_empty());
    }
}
