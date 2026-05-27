use serde_json::{Value, json};
use std::env;
use std::fmt;
use std::process;

const DEFAULT_BASE_URL: &str = "https://app.olyxai.io";

#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1).collect()).await {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

async fn run(args: Vec<String>) -> Result<(), CliError> {
    let invocation = Invocation::parse(args)?;

    match invocation.command {
        Command::Config => {
            let response = request_json(
                "GET",
                &invocation.base_url,
                "/api/v1/sdk/config",
                invocation.api_key.as_deref(),
                None,
                invocation.dry_run,
            )
            .await?;
            print_json(&response)
        }
        Command::Trace {
            metadata,
            local_ref,
        } => {
            let mut body =
                json!({ "metadata": metadata.unwrap_or_else(|| json!({ "source": "olyx-cli" })) });
            if let Some(local_ref) = local_ref {
                body["local_ref"] = Value::String(local_ref);
            }

            let response = request_json(
                "POST",
                &invocation.base_url,
                "/api/v1/traces",
                invocation.api_key.as_deref(),
                Some(&body),
                invocation.dry_run,
            )
            .await?;
            print_json(&response)
        }
        Command::Replay { trace_id } => {
            let body = json!({ "trace_id": trace_id });
            let response = request_json(
                "POST",
                &invocation.base_url,
                "/api/v1/replay",
                invocation.api_key.as_deref(),
                Some(&body),
                invocation.dry_run,
            )
            .await?;
            print_json(&response)
        }
        Command::Doctor => print_json(&json!({
            "base_url": invocation.base_url,
            "api_key_present": invocation.api_key.is_some(),
            "dry_run": invocation.dry_run,
            "secrets_boundary": "customer_environment"
        })),
        Command::Help => {
            println!("{}", usage());
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq)]
struct Invocation {
    command: Command,
    base_url: String,
    api_key: Option<String>,
    dry_run: bool,
}

impl Invocation {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut base_url =
            env::var("OLYX_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let mut api_key = env::var("OLYX_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let mut dry_run = false;
        let mut positionals = Vec::new();
        let mut command_seen = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--base-url" => {
                    i += 1;
                    base_url = args
                        .get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--base-url requires a value"))?;
                }
                "--api-key" => {
                    i += 1;
                    api_key = Some(
                        args.get(i)
                            .cloned()
                            .ok_or_else(|| CliError::usage("--api-key requires a value"))?,
                    );
                }
                "--dry-run" => dry_run = true,
                "-h" | "--help" => positionals.push("help".to_string()),
                other if other.starts_with("--") && !command_seen => {
                    return Err(CliError::usage(format!("unknown flag: {other}")));
                }
                other => {
                    command_seen = true;
                    positionals.push(other.to_string());
                }
            }
            i += 1;
        }

        let command = Command::parse(positionals)?;
        Ok(Self {
            command,
            base_url: normalize_base_url(&base_url)?,
            api_key,
            dry_run,
        })
    }
}

#[derive(Debug, PartialEq)]
enum Command {
    Config,
    Trace {
        metadata: Option<Value>,
        local_ref: Option<String>,
    },
    Replay {
        trace_id: String,
    },
    Doctor,
    Help,
}

impl Command {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        match args.first().map(String::as_str) {
            None | Some("help") => Ok(Self::Help),
            Some("config") => Ok(Self::Config),
            Some("doctor") => Ok(Self::Doctor),
            Some("trace") => parse_trace(args),
            Some("replay") => parse_replay(args),
            Some(other) => Err(CliError::usage(format!("unknown command: {other}"))),
        }
    }
}

fn parse_trace(args: Vec<String>) -> Result<Command, CliError> {
    let mut metadata = None;
    let mut local_ref = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--metadata" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| CliError::usage("--metadata requires JSON"))?;
                metadata = Some(
                    serde_json::from_str(raw)
                        .map_err(|e| CliError::usage(format!("invalid metadata JSON: {e}")))?,
                );
            }
            "--local-ref" => {
                i += 1;
                local_ref = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--local-ref requires a value"))?,
                );
            }
            other => return Err(CliError::usage(format!("unknown trace argument: {other}"))),
        }
        i += 1;
    }

    Ok(Command::Trace {
        metadata,
        local_ref,
    })
}

fn parse_replay(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() != 2 {
        return Err(CliError::usage("usage: olyx replay <trace_id>"));
    }

    Ok(Command::Replay {
        trace_id: args[1].clone(),
    })
}

async fn request_json(
    method: &str,
    base_url: &str,
    path: &str,
    api_key: Option<&str>,
    body: Option<&Value>,
    dry_run: bool,
) -> Result<Value, CliError> {
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "method": method,
            "url": format!("{base_url}{path}"),
            "body": body
        }));
    }

    let url = format!("{base_url}{path}");
    let client = reqwest::Client::new();
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
            "request returned HTTP {status}: {json_body}"
        )))
    }
}

fn normalize_base_url(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        Err(CliError::usage(
            "base URL must start with http:// or https://",
        ))
    }
}

fn print_json(value: &Value) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| CliError::message(e.to_string()))?
    );
    Ok(())
}

fn usage() -> &'static str {
    r#"olyx CLI

Usage:
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]
  olyx replay <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]
  olyx doctor [--base-url URL] [--api-key KEY] [--dry-run]

Environment:
  OLYX_BASE_URL  Defaults to https://app.olyxai.io
  OLYX_API_KEY   Project-scoped Olyx API key
"#
}

#[derive(Debug)]
struct CliError {
    message: String,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: format!("{}\n\n{}", message.into(), usage()),
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trace_with_metadata_and_local_ref() {
        let invocation = Invocation::parse(vec![
            "--base-url".into(),
            "http://localhost:3000/".into(),
            "trace".into(),
            "--metadata".into(),
            r#"{"source":"test"}"#.into(),
            "--local-ref".into(),
            "opaque:1".into(),
        ])
        .unwrap();

        assert_eq!(invocation.base_url, "http://localhost:3000");
        assert_eq!(
            invocation.command,
            Command::Trace {
                metadata: Some(json!({ "source": "test" })),
                local_ref: Some("opaque:1".into())
            }
        );
    }

    #[tokio::test]
    async fn dry_run_returns_request_shape() {
        let body = json!({ "trace_id": "trace_123" });
        let response = request_json(
            "POST",
            "https://app.olyxai.io",
            "/api/v1/replay",
            None,
            Some(&body),
            true,
        )
        .await
        .unwrap();

        assert_eq!(response["dry_run"], true);
        assert_eq!(response["method"], "POST");
        assert_eq!(response["url"], "https://app.olyxai.io/api/v1/replay");
        assert_eq!(response["body"], body);
    }
}
