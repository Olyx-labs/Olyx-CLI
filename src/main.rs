use olyx_core::{RuntimeVerifier, SignedEnvelope};
use serde_json::{Value, json};
use std::env;
use std::fmt;
use std::fs;
use std::process;

const DEFAULT_BASE_URL: &str = "https://app.olyxai.io";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1).collect()).await {
        eprintln!("error: {error}");
        process::exit(error.exit_code());
    }
}

async fn run(args: Vec<String>) -> Result<(), CliError> {
    let invocation = Invocation::parse(args)?;
    if matches!(
        &invocation.command,
        Command::Config | Command::Trace { .. } | Command::Replay { .. }
    ) {
        require_api_key(&invocation)?;
    }

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
        Command::Inspect => print_json(&json!({
            "base_url": invocation.base_url,
            "api_key_present": invocation.api_key.is_some(),
            "dry_run": invocation.dry_run,
            "secrets_boundary": "customer_environment",
            "core_runtime": runtime_env_status()
        })),
        Command::VerifyRuntime {
            license_path,
            policy_path,
            key_id,
            public_key_b64,
            public_key_path,
        } => {
            let status = verify_runtime(
                license_path.as_deref(),
                policy_path.as_deref(),
                key_id.as_deref(),
                public_key_b64.as_deref(),
                public_key_path.as_deref(),
            )?;
            print_json(&serde_json::to_value(status).map_err(|e| CliError::message(e.to_string()))?)
        }
        Command::Help => {
            println!("{}", usage());
            Ok(())
        }
        Command::Version => {
            println!("olyx {VERSION}");
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
                "-h" | "--help" => {
                    return Ok(Self::for_command(
                        Command::Help,
                        base_url,
                        api_key,
                        dry_run,
                    )?);
                }
                "-V" | "--version" => {
                    return Ok(Self::for_command(
                        Command::Version,
                        base_url,
                        api_key,
                        dry_run,
                    )?);
                }
                other if other.starts_with("--") && positionals.is_empty() => {
                    return Err(CliError::usage(format!("unknown flag: {other}")));
                }
                other => {
                    positionals.push(other.to_string());
                }
            }
            i += 1;
        }

        let command = Command::parse(positionals)?;
        Self::for_command(command, base_url, api_key, dry_run)
    }

    fn for_command(
        command: Command,
        base_url: String,
        api_key: Option<String>,
        dry_run: bool,
    ) -> Result<Self, CliError> {
        let base_url = if matches!(command, Command::Help | Command::Version) {
            DEFAULT_BASE_URL.to_string()
        } else {
            normalize_base_url(&base_url)?
        };

        Ok(Self {
            command,
            base_url,
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
    VerifyRuntime {
        license_path: Option<String>,
        policy_path: Option<String>,
        key_id: Option<String>,
        public_key_b64: Option<String>,
        public_key_path: Option<String>,
    },
    Inspect,
    Help,
    Version,
}

impl Command {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        match args.first().map(String::as_str) {
            None | Some("help") => Ok(Self::Help),
            Some("config") => Ok(Self::Config),
            Some("inspect") => Ok(Self::Inspect),
            Some("trace") => parse_trace(args),
            Some("replay") => parse_replay(args),
            Some("verify-runtime") => parse_verify_runtime(args),
            Some(other) => Err(CliError::usage(format!(
                "unknown command: {}",
                redact_argument(other)
            ))),
        }
    }
}

fn require_api_key(invocation: &Invocation) -> Result<(), CliError> {
    if invocation.api_key.is_some() || invocation.dry_run {
        Ok(())
    } else {
        Err(CliError::usage(
            "OLYX_API_KEY is required; export it or pass --api-key",
        ))
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

fn parse_verify_runtime(args: Vec<String>) -> Result<Command, CliError> {
    let mut license_path = None;
    let mut policy_path = None;
    let mut key_id = None;
    let mut public_key_b64 = None;
    let mut public_key_path = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--license" => {
                i += 1;
                license_path = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--license requires a path"))?,
                );
            }
            "--policy" => {
                i += 1;
                policy_path = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--policy requires a path"))?,
                );
            }
            "--key-id" => {
                i += 1;
                key_id = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--key-id requires a value"))?,
                );
            }
            "--public-key-b64" => {
                i += 1;
                public_key_b64 = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--public-key-b64 requires a value"))?,
                );
            }
            "--public-key-file" => {
                i += 1;
                public_key_path = Some(
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CliError::usage("--public-key-file requires a path"))?,
                );
            }
            other => {
                return Err(CliError::usage(format!(
                    "unknown verify-runtime argument: {other}"
                )));
            }
        }
        i += 1;
    }

    Ok(Command::VerifyRuntime {
        license_path,
        policy_path,
        key_id,
        public_key_b64,
        public_key_path,
    })
}

fn verify_runtime(
    license_path: Option<&str>,
    policy_path: Option<&str>,
    key_id: Option<&str>,
    public_key_b64: Option<&str>,
    public_key_path: Option<&str>,
) -> Result<olyx_core::RuntimeStatus, CliError> {
    let license_path = license_path
        .map(str::to_string)
        .or_else(|| env::var("OLYX_LICENSE_PATH").ok())
        .ok_or_else(|| {
            CliError::usage("missing license path; pass --license or set OLYX_LICENSE_PATH")
        })?;
    let policy_path = policy_path
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_PACK_PATH").ok())
        .ok_or_else(|| {
            CliError::usage("missing policy path; pass --policy or set OLYX_POLICY_PACK_PATH")
        })?;
    let key_id = key_id
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_KEY_ID").ok())
        .ok_or_else(|| {
            CliError::usage("missing key id; pass --key-id or set OLYX_POLICY_KEY_ID")
        })?;
    let public_key_b64 = public_key_b64
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_PUBLIC_KEY_B64").ok())
        .or_else(|| {
            let path = public_key_path
                .map(str::to_string)
                .or_else(|| env::var("OLYX_POLICY_PUBLIC_KEY_PATH").ok())?;
            fs::read_to_string(path).ok()
        })
        .ok_or_else(|| {
            CliError::usage(
                "missing public key; pass --public-key-b64, --public-key-file, or set OLYX_POLICY_PUBLIC_KEY_B64",
            )
        })?;

    let license = read_signed_envelope(&license_path)?;
    let policy_pack = read_signed_envelope(&policy_path)?;
    let verifier = RuntimeVerifier::from_public_key_b64(key_id, &public_key_b64)
        .map_err(|e| CliError::message(format!("runtime public key rejected: {e}")))?;
    let runtime = verifier
        .load_runtime(&license, &policy_pack)
        .map_err(|e| CliError::message(format!("runtime verification failed: {e}")))?;

    Ok(runtime.status())
}

fn read_signed_envelope(path: &str) -> Result<SignedEnvelope, CliError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| CliError::message(format!("failed to read {path}: {e}")))?;
    serde_json::from_str(&raw)
        .map_err(|e| CliError::message(format!("invalid signed envelope JSON at {path}: {e}")))
}

fn runtime_env_status() -> Value {
    json!({
        "license_path_present": env::var("OLYX_LICENSE_PATH").is_ok(),
        "policy_pack_path_present": env::var("OLYX_POLICY_PACK_PATH").is_ok(),
        "key_id_present": env::var("OLYX_POLICY_KEY_ID").is_ok(),
        "public_key_present": env::var("OLYX_POLICY_PUBLIC_KEY_B64").is_ok()
            || env::var("OLYX_POLICY_PUBLIC_KEY_PATH").is_ok(),
        "activation_command": "olyx verify-runtime"
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
            "api_key_present": api_key.is_some(),
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
            "request returned HTTP {status}: {}",
            truncate_for_error(&json_body.to_string())
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

fn redact_argument(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() > 80 {
        format!("{}...", trimmed.chars().take(80).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

fn truncate_for_error(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() > 500 {
        format!("{}...", trimmed.chars().take(500).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

fn usage() -> &'static str {
    r#"olyx CLI

Usage:
  olyx help
  olyx --version
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]
  olyx replay <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]
  olyx verify-runtime --license PATH --policy PATH --key-id KID (--public-key-b64 KEY | --public-key-file PATH)

Global Options:
  --base-url URL     Olyx API base URL. Defaults to https://app.olyxai.io
  --api-key KEY      Project-scoped Olyx API key. Prefer OLYX_API_KEY in production.
  --dry-run          Print the request shape without sending it.
  -h, --help         Print this help text.
  -V, --version      Print the CLI version.

Environment:
  OLYX_BASE_URL                Defaults to https://app.olyxai.io
  OLYX_API_KEY                 Project-scoped Olyx API key
  OLYX_LICENSE_PATH            Signed license envelope JSON
  OLYX_POLICY_PACK_PATH        Signed policy-pack envelope JSON
  OLYX_POLICY_KEY_ID           Expected Olyx signing key id
  OLYX_POLICY_PUBLIC_KEY_B64   Ed25519 public key, base64url or standard base64
  OLYX_POLICY_PUBLIC_KEY_PATH  File containing the public key
"#
}

#[derive(Debug)]
struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: format!("{}\n\n{}", message.into(), usage()),
            exit_code: 2,
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    fn exit_code(&self) -> i32 {
        self.exit_code
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

    #[test]
    fn parses_inspect_and_rejects_removed_alias() {
        let invocation = Invocation::parse(vec!["inspect".into()]).unwrap();
        assert_eq!(invocation.command, Command::Inspect);

        let removed_command = ["doc", "tor"].concat();
        let error = Invocation::parse(vec![removed_command]).unwrap_err();
        assert!(error.to_string().contains("unknown command"));
    }

    #[test]
    fn parses_global_flags_after_command() {
        let invocation = Invocation::parse(vec![
            "trace".into(),
            "--metadata".into(),
            r#"{"source":"test"}"#.into(),
            "--dry-run".into(),
            "--api-key".into(),
            "ak_test".into(),
            "--base-url".into(),
            "https://app.olyxai.io/".into(),
        ])
        .unwrap();

        assert_eq!(invocation.base_url, "https://app.olyxai.io");
        assert_eq!(invocation.api_key.as_deref(), Some("ak_test"));
        assert!(invocation.dry_run);
        assert_eq!(
            invocation.command,
            Command::Trace {
                metadata: Some(json!({ "source": "test" })),
                local_ref: None
            }
        );
    }

    #[test]
    fn parses_help_and_version_without_api_key() {
        let help = Invocation::parse(vec!["--help".into()]).unwrap();
        assert_eq!(help.command, Command::Help);

        let version = Invocation::parse(vec!["--version".into()]).unwrap();
        assert_eq!(version.command, Command::Version);
    }

    #[test]
    fn network_commands_require_api_key_unless_dry_run() {
        let invocation = Invocation {
            command: Command::Config,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: None,
            dry_run: false,
        };

        let error = require_api_key(&invocation).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("OLYX_API_KEY is required"));

        let dry_run = Invocation {
            dry_run: true,
            ..invocation
        };
        assert!(require_api_key(&dry_run).is_ok());
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
        assert_eq!(response["api_key_present"], false);
        assert_eq!(response["body"], body);
    }
}
