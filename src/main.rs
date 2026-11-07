#![forbid(unsafe_code)]
mod command;
mod error;
mod init;
mod input;
mod redact;
use command::{Command, Invocation, ParsedInvocation};
use error::CliError;
use redact::sanitize_json_for_output;
use serde_json::{json, Value};
use std::{env, process};
const VERSION: &str = env!("CARGO_PKG_VERSION");
#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1).collect()).await {
        eprintln!("error: {error}");
        process::exit(error.exit_code());
    }
}
async fn run(args: Vec<String>) -> Result<(), CliError> {
    let invocation = match Invocation::parse(args)? {
        ParsedInvocation::Run(invocation) => invocation,
        ParsedInvocation::Display(text) => {
            print!("{text}");
            return Ok(());
        }
    };
    if matches!(
        invocation.command,
        Command::Route { .. } | Command::Replay { .. }
    ) {
        // Drop the in-flight request before waiting for its trace guard to persist.
        let result = {
            let operation = run_invocation(invocation);
            tokio::pin!(operation);
            tokio::select! {
                // Install the interrupt handler before execution can create a trace.
                biased;
                signal = tokio::signal::ctrl_c() => {
                    signal.map_err(|e| CliError::message(format!("interrupt handler failed: {e}")))?;
                    None
                }
                result = &mut operation => Some(result),
            }
        };
        olyx_local_runtime::wait_for_cancelled_traces().await;
        return result.unwrap_or_else(|| Err(CliError::interrupted()));
    }
    run_invocation(invocation).await
}
async fn run_invocation(invocation: Invocation) -> Result<(), CliError> {
    match invocation.command {
        Command::Init {
            provider,
            model,
            alias,
            base_url,
            plugin,
        } => {
            if let Some(python) = plugin {
                return init::create_plugin(
                    &invocation.config,
                    &python,
                    model.as_deref().expect("parser requires model"),
                    &alias,
                );
            }
            init::create(
                &invocation.config,
                provider.as_deref(),
                model.as_deref(),
                &alias,
                base_url.as_deref(),
            )
        }
        Command::Serve {
            host,
            port,
            max_concurrency,
        } => {
            let listen = std::net::SocketAddr::new(host, port);
            let config = load_config(&invocation.config)?;
            let api_key = env::var("OLYX_SERVER_API_KEY").ok();
            olyx_local_runtime::server::serve(config, listen, api_key, usize::from(max_concurrency))
                .await
                .map_err(|e| CliError::message(e.to_string()))
        }
        Command::Route {
            model,
            input,
            dry_run,
        } => {
            let path = &invocation.config;
            let config = load_config(path)?;
            let mut body = input::read_request(&input, config.policy.max_body_bytes)?;
            if let Some(model) = model {
                body["model"] = json!(model);
            }
            if dry_run {
                return print_json(&json!({"local":true,"request":body,"config":path}));
            }
            let state = olyx_local_runtime::State::new(config)
                .map_err(|e| CliError::message(e.to_string()))?;
            let streaming = body["stream"].as_bool() == Some(true);
            let output = state
                .route_with_events(body, &mut |event| {
                    write_json(&mut std::io::stdout().lock(), &event, false)
                        .map_err(|_| olyx_local_runtime::Failure::new(500, "output_write_failed"))
                })
                .await
                .map_err(|e| CliError::message(e.to_string()))?;
            if streaming {
                write_json(
                    &mut std::io::stdout().lock(),
                    &json!({"type":"completed","trace_id":output["trace_id"],"output":output["output"]}),
                    false,
                )
                .map_err(|e| CliError::message(e.to_string()))
            } else {
                print_json(&output)
            }
        }
        Command::Doctor { probe } => {
            let mut report = olyx_local_runtime::doctor::run(&invocation.config, probe).await;
            report["installation"] = json!({"version": VERSION, "storage": "local"});
            print_json(&report)?;
            if report["ok"] == true {
                Ok(())
            } else {
                Err(CliError::message(
                    "doctor found setup problems; see checks above",
                ))
            }
        }
        Command::Debug { trace_id } => {
            let state = load_state(&invocation.config)?;
            let output = state
                .debug(trace_id.as_deref())
                .await
                .map_err(|e| CliError::message(e.to_string()))?;
            if let Some(diagnostic) = output.get("diagnostic") {
                eprintln!(
                    "{}",
                    crate::redact::sanitize_text_for_output(&format!(
                        "Failure: {}\n{}\nProvider: {} | Alias: {} | Upstream: {}\nFallback: {}\nHint: {}\nTrace: {}",
                        diagnostic["code"].as_str().unwrap_or("unknown_error"),
                        diagnostic["message"]
                            .as_str()
                            .unwrap_or("Inspect failed steps."),
                        diagnostic["provider"].as_str().unwrap_or("not recorded"),
                        diagnostic["alias"].as_str().unwrap_or("not recorded"),
                        diagnostic["upstream_status"],
                        diagnostic["fallback"].as_str().unwrap_or("inspect steps"),
                        diagnostic["hint"]
                            .as_str()
                            .unwrap_or("Inspect failed steps."),
                        output["external_id"].as_str().unwrap_or("unknown")
                    ))
                );
            }
            print_json(&output)
        }
        Command::Check { file } => {
            load_config(&file)?;
            println!("Local configuration is valid");
            Ok(())
        }
        Command::Replay {
            trace_id,
            models,
            force_model,
            dry_run,
        } => {
            let path = invocation.config;
            if dry_run {
                return print_json(
                    &json!({"local": true, "config": path, "trace_id": trace_id, "compare_models": models, "force_model": force_model}),
                );
            }
            let state = load_state(&path)?;
            let output = state
                .replay(&trace_id, force_model, models)
                .await
                .map_err(|e| CliError::message(e.to_string()))?;
            print_json(&output)?;
            if output["status"] != "completed" {
                return Err(CliError::message("Comparison contains failed requests; inspect comparison.replays in the JSON output."));
            }
            Ok(())
        }
    }
}
fn load_config(path: &str) -> Result<olyx_local_runtime::config::Config, CliError> {
    olyx_local_runtime::config::Config::load(path).map_err(|e| CliError::message(e.to_string()))
}
fn load_state(path: &str) -> Result<std::sync::Arc<olyx_local_runtime::State>, CliError> {
    olyx_local_runtime::State::new(load_config(path)?).map_err(|e| CliError::message(e.to_string()))
}
fn write_json(
    writer: &mut impl std::io::Write,
    value: &Value,
    pretty: bool,
) -> std::io::Result<()> {
    let value = sanitize_json_for_output(value);
    if pretty {
        serde_json::to_writer_pretty(&mut *writer, &value)
    } else {
        serde_json::to_writer(&mut *writer, &value)
    }
    .map_err(|error| {
        let kind = error.io_error_kind().unwrap_or(std::io::ErrorKind::Other);
        std::io::Error::new(kind, error)
    })?;
    writer.write_all(b"\n")?;
    writer.flush()
}
fn print_json(value: &Value) -> Result<(), CliError> {
    write_json(&mut std::io::stdout().lock(), value, true)
        .map_err(|e| CliError::message(e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_output_redacts_secrets_and_preserves_line_framing() {
        for pretty in [false, true] {
            let mut bytes = Vec::new();
            write_json(
                &mut bytes,
                &json!({"authorization":"Bearer secret", "ok":true}),
                pretty,
            )
            .unwrap();
            assert!(bytes.ends_with(b"\n"));
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["authorization"], "[REDACTED]");
            assert_eq!(value["ok"], true);
            if !pretty {
                assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
            }
        }
    }
    #[test]
    fn json_output_propagates_closed_pipe_errors() {
        struct Closed;
        impl std::io::Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_json(&mut Closed, &json!({}), false)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }
    #[test]
    fn configuration_flag_works_after_command() {
        let parsed =
            Invocation::parse(vec!["debug".into(), "--config".into(), "local.json".into()])
                .unwrap();
        let ParsedInvocation::Run(invocation) = parsed else {
            panic!("expected command")
        };
        assert_eq!(invocation.config, "local.json");
    }
}
