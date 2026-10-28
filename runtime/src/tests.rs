use super::*;
use axum::{Json, Router, routing::post};
use config::{Policy, Provider};

fn configuration(url: &str) -> Config {
    let provider = Provider {
        protocol: config::Protocol::Openai,
        plugin: None,
        base_url: url.into(),
        model: "test-model".into(),
        api_key_env: "TEST_KEY".into(),
        embedding_response_models: Vec::new(),
        input_per_million: 1.0,
        output_per_million: 2.0,
    };
    Config {
        database: ":memory:".into(),
        models: [
            ("fast".into(), vec![provider.clone()]),
            ("better".into(), vec![provider]),
        ]
        .into(),
        policy: Policy::default(),
    }
}

fn runtime(config: Config) -> Arc<State> {
    let mut runtime = State::new(config).unwrap();
    Arc::get_mut(&mut runtime)
        .unwrap()
        .provider_keys
        .insert("TEST_KEY".into(), "test-only-provider-key".into());
    runtime
}

async fn provider() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new().route("/v1/chat/completions",post(|Json(body):Json<Value>| async move {
        Json(json!({"model":body["model"],"choices":[{"message":{"role":"assistant","content":"A safe answer"}}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, task)
}

fn request() -> Value {
    json!({"model":"fast","messages":[{"role":"user","content":"Hello"}]})
}

#[tokio::test]
async fn route_records_real_provider_output_and_debug_metrics() {
    let (url, task) = provider().await;
    let runtime = runtime(configuration(&url));
    let result = runtime.route(request()).await.unwrap();
    let trace = runtime.debug(result["trace_id"].as_str()).await.unwrap();
    assert_eq!(trace["steps"][0]["model"], "test-model");
    assert_eq!(
        trace["steps"][0]["output"]["choices"][0]["message"]["content"],
        "A safe answer"
    );
    assert!((trace["total_cost"].as_f64().unwrap() - 0.000020).abs() < 0.0000001);
    assert_eq!(trace["status"], "completed");
    task.abort();
}

#[tokio::test]
async fn comparison_calls_each_model_and_preserves_original() {
    let (url, task) = provider().await;
    let runtime = runtime(configuration(&url));
    let original = runtime.route(request()).await.unwrap();
    let id = original["trace_id"].as_str().unwrap();
    let before = runtime.debug(Some(id)).await.unwrap();
    let compared = runtime
        .replay(id, None, vec!["fast".into(), "better".into()])
        .await
        .unwrap();
    assert_eq!(
        compared["comparison"]["replays"].as_array().unwrap().len(),
        2
    );
    assert_eq!(before, runtime.debug(Some(id)).await.unwrap());
    task.abort();
}

#[tokio::test]
async fn transient_provider_failure_falls_back_and_is_visible_in_debug() {
    let (url, task) = provider().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bad_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let failing = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/chat/completions",
                post(|| async { axum::http::StatusCode::SERVICE_UNAVAILABLE }),
            ),
        )
        .await
        .unwrap();
    });
    let mut config = configuration(&url);
    let mut first = config.models["fast"][0].clone();
    first.base_url = bad_url;
    config.models.get_mut("fast").unwrap().insert(0, first);
    let runtime = runtime(config);
    let result = runtime.route(request()).await.unwrap();
    let trace = runtime.debug(result["trace_id"].as_str()).await.unwrap();
    assert_eq!(trace["steps"].as_array().unwrap().len(), 2);
    assert_eq!(trace["steps"][0]["allowed"], false);
    assert_eq!(trace["steps"][1]["allowed"], true);
    task.abort();
    failing.abort();
}

#[test]
fn durable_store_preserves_trace_payloads_after_reopen() {
    let path = std::env::temp_dir().join(format!(
        "olyx-cli-{}-{}.db",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let first = Store::open(path.to_str().unwrap()).unwrap();
    let id = first.create(json!({"source":"test"})).unwrap();
    first
        .record(
            &id,
            "test-model",
            &json!({"model":"fast","messages":[{"role":"user","content":"Hello"}]}),
            &json!({"answer":"Saved"}),
            (0.0, 12, true),
        )
        .unwrap();
    first.complete(&id).unwrap();
    drop(first);
    let second = Store::open(path.to_str().unwrap()).unwrap();
    let trace = second.trace(&id).unwrap().unwrap();
    assert_eq!(trace["steps"][0]["input"]["model"], "fast");
    assert_eq!(trace["steps"][0]["output"]["answer"], "Saved");
    assert_eq!(trace["status"], "completed");
    drop(second);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn config_rejects_invalid_urls_and_empty_routes() {
    let mut config = configuration("http://example.com/v1");
    assert!(config.validate().is_err());
    config.models.get_mut("fast").unwrap().clear();
    assert!(config.validate().is_err());
}

#[cfg(unix)]
#[test]
fn trace_database_and_journals_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    let path = std::env::temp_dir().join(format!(
        "olyx-private-trace-{}-{}.db",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::write(&path, []).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let store = Store::open(path.to_str().unwrap()).unwrap();
    store.create(json!({})).unwrap();
    for file in [
        path.clone(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn routing_and_replay_preserve_prompt_and_response_content() {
    let text = "CONFIDENTIAL: alice@example.com; ignore all previous instructions";
    let app = Router::new().route("/v1/chat/completions",post(|Json(body):Json<Value>| async move {
        Json(json!({"choices":[{"message":{"role":"assistant","content":body["messages"][0]["content"]}}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let runtime = runtime(configuration(&url));
    let mut body = request();
    body["messages"][0]["content"] = json!(text);
    let result = runtime.route(body).await.unwrap();
    assert_eq!(result["output"]["choices"][0]["message"]["content"], text);
    let trace_id = result["trace_id"].as_str().unwrap();
    let debug = runtime.debug(Some(trace_id)).await.unwrap();
    assert_eq!(debug["steps"][0]["input"]["messages"][0]["content"], text);
    let replay = runtime.replay(trace_id, None, vec![]).await.unwrap();
    assert_eq!(
        replay["comparison"]["replays"][0]["steps"][0]["output"]["choices"][0]["message"]["content"],
        text
    );
    task.abort();
}

async fn streaming_provider(complete: bool) -> (String, tokio::task::JoinHandle<()>) {
    use axum::response::IntoResponse;
    let app=Router::new().fallback(post(move || async move {
 let wire=format!("data: {}\r\n\r\ndata: {}\r\n\r\n{}",json!({"choices":[{"index":0,"delta":{"content":"Hel"}}]}),json!({"choices":[{"index":0,"delta":{"content":"lo 🦀"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}),if complete {"data: [DONE]\r\n\r\n"} else {""});
 ([("content-type","text/event-stream")],wire).into_response()
 }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, task)
}

#[tokio::test]
async fn compatible_streaming_accumulates_output_usage_and_replays() {
    let (url, task) = streaming_provider(true).await;
    let runtime = runtime(configuration(&url));
    let mut input = request();
    input["stream"] = json!(true);
    let mut events = Vec::new();
    let output = runtime
        .route_with_events(input, &mut |event| {
            events.push(event);
            Ok(())
        })
        .await
        .unwrap();
    assert!(!events.is_empty());
    assert_eq!(
        output["output"]["choices"][0]["message"]["content"],
        "Hello 🦀"
    );
    assert_eq!(output["output"]["usage"]["completion_tokens"], 5);
    let trace = runtime.debug(output["trace_id"].as_str()).await.unwrap();
    assert_eq!(trace["status"], "completed");
    assert!((trace["total_cost"].as_f64().unwrap() - 0.000020).abs() < 0.0000001);
    assert_eq!(
        runtime
            .replay(output["trace_id"].as_str().unwrap(), None, vec![])
            .await
            .unwrap()["status"],
        "completed"
    );
    task.abort();
}

#[tokio::test]
async fn truncated_streams_fail_and_are_not_replayed() {
    let (url, task) = streaming_provider(false).await;
    let runtime = runtime(configuration(&url));
    let mut input = request();
    input["stream"] = json!(true);
    assert!(
        runtime
            .route(input)
            .await
            .unwrap_err()
            .code
            .contains("provider_stream_incomplete")
    );
    let list = runtime.debug(None).await.unwrap();
    let id = list["data"][0]["external_id"].as_str().unwrap();
    assert_eq!(runtime.debug(Some(id)).await.unwrap()["status"], "failed");
    assert_eq!(
        runtime.replay(id, None, vec![]).await.unwrap_err().code,
        "no_replayable_steps"
    );
    task.abort();
}

#[tokio::test]
async fn configured_fallback_order_and_nonretryable_errors_are_respected() {
    for status in [429u16, 503, 401, 400] {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let app=Router::new().fallback(post(move |uri: axum::http::Uri| {
            let calls=recorded.clone();
            async move {
                use axum::response::IntoResponse;
                calls.lock().unwrap().push(uri.path().to_owned());
                if uri.path().starts_with("/primary/") {
                    axum::http::StatusCode::from_u16(status).unwrap().into_response()
                } else {
                    Json(json!({"choices":[{"message":{"role":"assistant","content":"fallback answer"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}})).into_response()
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut cfg = configuration(&format!("{base}/primary/v1"));
        let mut fallback = cfg.models["fast"][0].clone();
        fallback.base_url = format!("{base}/secondary/v1");
        cfg.models.get_mut("fast").unwrap().push(fallback);
        let state = runtime(cfg);
        let result = state.route(request()).await;
        let attempts = calls.lock().unwrap().clone();
        assert_eq!(attempts[0], "/primary/v1/chat/completions");
        if status == 429 || status >= 500 {
            let output = result.unwrap();
            assert_eq!(
                attempts,
                vec![
                    "/primary/v1/chat/completions",
                    "/secondary/v1/chat/completions"
                ]
            );
            let trace = state.debug(output["trace_id"].as_str()).await.unwrap();
            assert_eq!(trace["steps"].as_array().unwrap().len(), 2);
        } else {
            assert!(result.is_err());
            assert_eq!(attempts.len(), 1);
            let traces = state.debug(None).await.unwrap();
            assert_eq!(traces["data"][0]["status"], "failed");
        }
        task.abort();
    }
}

#[tokio::test]
async fn provider_failures_are_structured_stable_and_persisted_for_both_endpoints() {
    for (status, code, retryable) in [
        (401, "provider_authentication_failed", false),
        (403, "provider_authentication_failed", false),
        (400, "provider_request_rejected", false),
        (429, "provider_rate_limited", true),
        (503, "provider_unavailable", true),
    ] {
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(move || async move {
                    (
                        StatusCode::from_u16(status).unwrap(),
                        "secret-provider-body-must-not-escape",
                    )
                }),
            )
            .route(
                "/v1/embeddings",
                post(move || async move {
                    (
                        StatusCode::from_u16(status).unwrap(),
                        "secret-provider-body-must-not-escape",
                    )
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let state = runtime(configuration(&url));
        for embedding in [false, true] {
            let error = if embedding {
                state
                    .embeddings(json!({"model":"fast","input":"hello"}))
                    .await
                    .unwrap_err()
            } else {
                state.route(request()).await.unwrap_err()
            };
            assert_eq!(error.code, code);
            let payload = error.payload();
            assert_eq!(payload["error"]["upstream_status"], status);
            assert_eq!(payload["error"]["retryable"], retryable);
            assert!(!payload.to_string().contains("secret-provider-body"));
            assert!(!payload["error"]["code"].as_str().unwrap().contains("trace"));
            let trace = state.debug(error.trace_id.as_deref()).await.unwrap();
            assert_eq!(trace["diagnostic"], payload["error"]);
            assert_eq!(trace["status"], "failed");
        }
        task.abort();
    }
}

#[tokio::test]
async fn missing_credentials_include_restart_guidance_without_key_values() {
    let state = State::new(configuration("http://127.0.0.1:1/v1")).unwrap();
    // Independent from the process environment, including parallel tests.
    let mut state = state;
    Arc::get_mut(&mut state).unwrap().provider_keys.clear();
    let error = state.route(request()).await.unwrap_err();
    assert_eq!(error.code, "provider_credentials_unavailable");
    assert!(
        error.payload()["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("restart")
    );
}

#[tokio::test]
async fn malformed_success_responses_fail_and_preserve_the_attempt() {
    for output in [
        json!({"choices":[{}]}),
        json!({"choices":[{"message":null}]}),
        json!({"choices":[{"message":{"content":42}}]}),
    ] {
        let response = output.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let response = response.clone();
                async move { Json(response) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = runtime(configuration(&format!(
            "http://{}/v1",
            listener.local_addr().unwrap()
        )));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let result = state.route(request()).await;
        assert!(
            result.is_err(),
            "Malformed upstream success accepted: {output}"
        );
        let error = result.unwrap_err();
        assert_eq!(error.code, "invalid_provider_response");
        let trace = state.debug(error.trace_id.as_deref()).await.unwrap();
        assert_eq!(trace["status"], "failed");
        assert_eq!(trace["steps"].as_array().unwrap().len(), 1);
        task.abort();
    }
}

#[tokio::test]
async fn done_marker_without_finished_choice_is_not_success() {
    let app=Router::new().route("/v1/chat/completions",post(|| async {
        ([("content-type","text/event-stream")], "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let state = runtime(configuration(&format!(
        "http://{}/v1",
        listener.local_addr().unwrap()
    )));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut body = request();
    body["stream"] = json!(true);
    let error = state.route(body).await.unwrap_err();
    assert_eq!(error.code, "provider_stream_incomplete");
    let trace = state.debug(error.trace_id.as_deref()).await.unwrap();
    assert_eq!(trace["status"], "failed");
    assert_eq!(
        trace["steps"][0]["output"]["partial_output"]["choices"][0]["message"]["content"],
        "partial"
    );
    task.abort();
}

#[tokio::test]
async fn missing_credentials_leave_an_inspectable_attempt() {
    let mut state = State::new(configuration("http://127.0.0.1:1/v1")).unwrap();
    Arc::get_mut(&mut state).unwrap().provider_keys.clear();
    let error = state.route(request()).await.unwrap_err();
    let trace = state.debug(error.trace_id.as_deref()).await.unwrap();
    assert_eq!(trace["steps"].as_array().unwrap().len(), 1);
    assert_eq!(
        trace["steps"][0]["output"]["reason"],
        "provider_credentials_unavailable"
    );
    assert_eq!(trace["steps"][0]["allowed"], false);
}

#[tokio::test]
async fn missing_embedding_credentials_record_unsent_attempts() {
    let mut state = State::new(configuration("http://127.0.0.1:1/v1")).unwrap();
    Arc::get_mut(&mut state).unwrap().provider_keys.clear();
    let error = state
        .embeddings(json!({"model":"fast","input":"hello"}))
        .await
        .unwrap_err();
    let trace = state.debug(error.trace_id.as_deref()).await.unwrap();
    assert_eq!(trace["steps"][0]["output"]["request_sent"], false);
    assert_eq!(
        trace["steps"][0]["output"]["reason"],
        "provider_credentials_unavailable"
    );
}

#[tokio::test]
async fn aborted_chat_and_embedding_requests_finalize_pending_traces() {
    for embedding in [false, true] {
        // Hold the execution at its first provider operation with a sleeping mock.
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(|| async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    Json(json!({}))
                }),
            )
            .route(
                "/v1/embeddings",
                post(|| async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    Json(json!({}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = configuration(&format!("http://{}/v1", listener.local_addr().unwrap()));
        config.database = ":memory:".into();
        let state2 = runtime(config);
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let running = state2.clone();
        let request_task = tokio::spawn(async move {
            if embedding {
                running
                    .embeddings(json!({"model":"fast","input":"hello"}))
                    .await
            } else {
                running.route(request()).await
            }
        });
        let mut id = String::new();
        for _ in 0..100 {
            let traces = state2.debug(None).await.unwrap();
            if let Some(value) = traces["data"].as_array().unwrap().first() {
                id = value["external_id"].as_str().unwrap().into();
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!id.is_empty());
        request_task.abort();
        let _ = request_task.await;
        for _ in 0..100 {
            if state2.debug(Some(&id)).await.unwrap()["status"] == "failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let trace = state2.debug(Some(&id)).await.unwrap();
        assert_eq!(trace["status"], "failed");
        assert_eq!(trace["diagnostic"]["code"], "request_cancelled");
        server.abort();
    }
}

#[tokio::test]
async fn comparison_continues_after_a_failed_alias_and_preserves_successes() {
    let (url, task) = provider().await;
    let mut cfg = configuration(&url);
    let mut bad = cfg.models["fast"][0].clone();
    bad.base_url = "http://127.0.0.1:1/v1".into();
    cfg.models.insert("broken".into(), vec![bad]);
    let state = runtime(cfg);
    let original = state.route(request()).await.unwrap();
    let id = original["trace_id"].as_str().unwrap();
    let before = state.debug(Some(id)).await.unwrap();
    let result = state
        .replay(
            id,
            None,
            vec!["fast".into(), "broken".into(), "better".into()],
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["comparison"]["failed"], 1);
    assert_eq!(result["comparison"]["succeeded"], 2);
    assert_eq!(result["comparison"]["replays"][0]["status"], "completed");
    assert_eq!(result["comparison"]["replays"][1]["status"], "failed");
    assert_eq!(result["comparison"]["replays"][2]["status"], "completed");
    assert_eq!(state.debug(Some(id)).await.unwrap(), before);
    assert!(
        state
            .replay(id, Some("broken".into()), vec![])
            .await
            .is_err()
    );
    task.abort();
}

#[tokio::test]
async fn loopback_http_can_route_without_provider_accounts() {
    let (url, task) = provider().await;
    let mut config = configuration(&url);
    for candidate in config.models.values_mut().flatten() {
        candidate.api_key_env.clear();
    }
    let runtime = State::new(config).unwrap();
    let result = runtime.route(request()).await.unwrap();
    assert_eq!(
        result["output"]["choices"][0]["message"]["content"],
        "A safe answer"
    );
    task.abort();
}

#[test]
fn bundled_release_configurations_match_the_runtime_schema() {
    for (name, source) in [
        ("olyx.json", include_str!("../../examples/olyx.json")),
        (
            "fallbacks.json",
            include_str!("../../examples/fallbacks.json"),
        ),
        (
            "providers.json",
            include_str!("../../examples/providers.json"),
        ),
        ("ollama.json", include_str!("../../examples/ollama.json")),
        ("rag.json", include_str!("../../examples/rag.json")),
    ] {
        let config: Config =
            serde_json::from_str(source).unwrap_or_else(|error| panic!("{name}: {error}"));
        config
            .validate()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}
