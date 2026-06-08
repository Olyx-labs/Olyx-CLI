use olyx_core::{RuntimeMode, RuntimeVerifier, SignedEnvelope};
use serde_json::{Value, json};
use std::env;
use std::fmt;
use std::fs;
use std::process;

const DEFAULT_BASE_URL: &str = "https://app.olyxai.io";
const MAX_SIGNED_ENVELOPE_FILE_BYTES: u64 = 3 * 1024 * 1024;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REDACTED: &str = "[REDACTED]";

#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1).collect()).await {
        eprintln!("error: {error}");
        process::exit(error.exit_code());
    }
}

async fn run(args: Vec<String>) -> Result<(), CliError> {
    // Keep the top-level dispatcher thin: parse once, then hand off to a
    // single command-specific branch so the public CLI remains easy to read.
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
            "api_key_present": api_key_present(invocation.api_key.as_deref()),
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
        Command::Help(topic) => {
            println!("{}", help_text(topic));
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
                "-h" | "--help" if positionals.is_empty() => {
                    return Self::for_command(
                        Command::Help(HelpTopic::General),
                        base_url,
                        api_key,
                        dry_run,
                    );
                }
                "-h" | "--help" => positionals.push(args[i].clone()),
                "-V" | "--version" => {
                    return Self::for_command(Command::Version, base_url, api_key, dry_run);
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
        let base_url = if matches!(command, Command::Help(_) | Command::Version) {
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
    Help(HelpTopic),
    Version,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum HelpTopic {
    General,
    Inspect,
    Config,
    Trace,
    Replay,
    VerifyRuntime,
}

impl Command {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        match args.first().map(String::as_str) {
            None => Ok(Self::Help(HelpTopic::General)),
            Some("help") => parse_help(args),
            Some("config") => parse_fixed_arity_help(args, HelpTopic::Config, Self::Config),
            Some("inspect") => parse_fixed_arity_help(args, HelpTopic::Inspect, Self::Inspect),
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

fn parse_help(args: Vec<String>) -> Result<Command, CliError> {
    match args.as_slice() {
        [_] => Ok(Command::Help(HelpTopic::General)),
        [_, topic] => Ok(Command::Help(parse_help_topic(topic)?)),
        _ => Err(CliError::usage(
            "usage: olyx help [inspect|config|trace|replay|verify-runtime]",
        )),
    }
}

fn parse_help_topic(raw: &str) -> Result<HelpTopic, CliError> {
    match raw {
        "inspect" => Ok(HelpTopic::Inspect),
        "config" => Ok(HelpTopic::Config),
        "trace" => Ok(HelpTopic::Trace),
        "replay" => Ok(HelpTopic::Replay),
        "verify-runtime" => Ok(HelpTopic::VerifyRuntime),
        other => Err(CliError::usage(format!(
            "unknown help topic: {}",
            redact_argument(other)
        ))),
    }
}

fn parse_fixed_arity_help(
    args: Vec<String>,
    help_topic: HelpTopic,
    command: Command,
) -> Result<Command, CliError> {
    // `config` and `inspect` intentionally have a narrow surface: either the
    // command itself or a help flag. Extra args are usually typos and should
    // fail fast instead of being ignored.
    if args.len() == 1 {
        Ok(command)
    } else if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        Ok(Command::Help(help_topic))
    } else if let Some(name) = args.first() {
        Err(CliError::usage(format!(
            "usage: olyx {name} [--base-url URL] [--api-key KEY] [--dry-run]"
        )))
    } else {
        unreachable!()
    }
}

fn require_api_key(invocation: &Invocation) -> Result<(), CliError> {
    if api_key_present(invocation.api_key.as_deref()) || invocation.dry_run {
        Ok(())
    } else {
        Err(CliError::usage(
            "OLYX_API_KEY is required; export it or pass --api-key",
        ))
    }
}

fn api_key_present(api_key: Option<&str>) -> bool {
    api_key.is_some_and(|value| !value.trim().is_empty())
}

fn parse_trace(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help(HelpTopic::Trace));
    }

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
    if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help(HelpTopic::Replay));
    }

    if args.len() != 2 {
        return Err(CliError::usage("usage: olyx replay <trace_id>"));
    }

    Ok(Command::Replay {
        trace_id: args[1].clone(),
    })
}

fn parse_verify_runtime(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help(HelpTopic::VerifyRuntime));
    }

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
    // Runtime activation fails closed: the CLI only succeeds when both the
    // signed artifacts and the licensed runtime modes explicitly allow CLI use.
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
        .map(Ok)
        .or_else(|| {
            let path = public_key_path
                .map(str::to_string)
                .or_else(|| env::var("OLYX_POLICY_PUBLIC_KEY_PATH").ok())?;
            Some(read_public_key_file(&path))
        })
        .transpose()?
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
        .load_runtime_for_mode(&license, &policy_pack, RuntimeMode::Cli)
        .map_err(|e| CliError::message(format!("runtime verification failed: {e}")))?;
    let status = runtime.status();
    if !status.runtime_modes.contains(&RuntimeMode::Cli) {
        return Err(CliError::message(
            "runtime verification failed: license does not include CLI runtime mode",
        ));
    }

    Ok(status)
}

fn read_signed_envelope(path: &str) -> Result<SignedEnvelope, CliError> {
    let metadata = fs::metadata(path).map_err(|e| {
        CliError::message(format!(
            "failed to inspect signed envelope file {}: {e}",
            truncate_path(path)
        ))
    })?;
    if metadata.len() > MAX_SIGNED_ENVELOPE_FILE_BYTES {
        return Err(CliError::message(format!(
            "signed envelope file {} exceeds {} bytes",
            truncate_path(path),
            MAX_SIGNED_ENVELOPE_FILE_BYTES
        )));
    }

    let raw = fs::read_to_string(path).map_err(|e| {
        CliError::message(format!(
            "failed to read signed envelope file {}: {e}",
            truncate_path(path)
        ))
    })?;
    serde_json::from_str(&raw).map_err(|e| {
        CliError::message(format!(
            "invalid signed envelope JSON at {}: {e}",
            truncate_path(path)
        ))
    })
}

fn read_public_key_file(path: &str) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|e| {
        CliError::message(format!(
            "failed to read public key file {}: {e}",
            truncate_path(path)
        ))
    })
}

fn truncate_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.len() > 120 {
        format!("{}...", trimmed.chars().take(120).collect::<String>())
    } else {
        trimmed.to_string()
    }
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

fn normalize_base_url(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else if trimmed.starts_with("http://") {
        // Allow http only for loopback addresses (local development and CI smoke checks).
        let host = trimmed.trim_start_matches("http://");
        let host = host.split('/').next().unwrap_or("").split(':').next().unwrap_or("");
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

fn print_json(value: &Value) -> Result<(), CliError> {
    let sanitized = sanitize_json_for_output(value);
    println!(
        "{}",
        serde_json::to_string_pretty(&sanitized).map_err(|e| CliError::message(e.to_string()))?
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
    let trimmed = sanitize_text_for_output(raw);
    let trimmed = trimmed.trim();
    if trimmed.len() > 500 {
        format!("{}...", trimmed.chars().take(500).collect::<String>())
    } else {
        trimmed.to_string()
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

fn sanitize_json_for_output(value: &Value) -> Value {
    // Redact recursively because responses, trace bodies, and dry-run payloads
    // can carry secrets several levels deep in nested objects or arrays.
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let sanitized = if is_sensitive_key(key) {
                        Value::String(REDACTED.to_string())
                    } else {
                        sanitize_json_for_output(value)
                    };
                    (key.clone(), sanitized)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_json_for_output).collect()),
        Value::String(text) => Value::String(sanitize_text_for_output(text)),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    // Normalize to lowercase with underscores so both snake_case and camelCase
    // key names are caught (e.g. "api_key" and "apiKey" both normalize to "apikey").
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

fn sanitize_text_for_output(raw: &str) -> String {
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
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0;

    while let Some(relative) = raw[cursor..].find("Bearer ") {
        let start = cursor + relative;
        let token_start = start + "Bearer ".len();
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
    raw.chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        .map(char::len_utf8)
        .sum()
}

fn help_text(topic: HelpTopic) -> &'static str {
    // Keep help bodies as constants so the public shell help and the snapshot
    // tests stay in lockstep as the CLI surface evolves.
    match topic {
        HelpTopic::General => GENERAL_HELP,
        HelpTopic::Inspect => INSPECT_HELP,
        HelpTopic::Config => CONFIG_HELP,
        HelpTopic::Trace => TRACE_HELP,
        HelpTopic::Replay => REPLAY_HELP,
        HelpTopic::VerifyRuntime => VERIFY_RUNTIME_HELP,
    }
}

const GENERAL_HELP: &str = r#"olyx CLI

Usage:
  olyx help
  olyx help <inspect|config|trace|replay|verify-runtime>
  olyx --version
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]
  olyx replay <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]
  olyx verify-runtime --license PATH --policy PATH --key-id KID (--public-key-b64 KEY | --public-key-file PATH)

Global Options:
  --base-url URL     Olyx API base URL. Defaults to https://app.olyxai.io
  --api-key KEY      Project-scoped Olyx API key. Prefer OLYX_API_KEY in production;
                     passing a key via --api-key exposes it in the process table.
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
"#;

const INSPECT_HELP: &str = r#"olyx inspect

Usage:
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]

Print local CLI and private-runtime environment status without making network
requests.
"#;

const CONFIG_HELP: &str = r#"olyx config

Usage:
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]

Fetch the project configuration contract from Olyx Cloud, including distribution,
guardrails, and replay mode.
"#;

const TRACE_HELP: &str = r#"olyx trace

Usage:
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]

Create a trace envelope in Olyx Cloud using metadata only or an opaque local
reference.
"#;

const REPLAY_HELP: &str = r#"olyx replay

Usage:
  olyx replay <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]

Queue replay work for an existing trace id.
"#;

const VERIFY_RUNTIME_HELP: &str = r#"olyx verify-runtime

Usage:
  olyx verify-runtime --license PATH --policy PATH --key-id KID (--public-key-b64 KEY | --public-key-file PATH)

Verify the signed runtime license and policy-pack envelopes required to activate
the private compiled core.
"#;

#[derive(Debug)]
struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        let message = sanitize_text_for_output(&message.into());
        Self {
            message: format!("{message}\n\n{GENERAL_HELP}"),
            exit_code: 2,
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: sanitize_text_for_output(&message.into()),
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

    fn fake_olyx_key() -> String {
        ["ak", "_live_", "1234567890abcdef"].concat()
    }

    fn fake_openai_key() -> String {
        ["sk", "-proj-", "1234567890abcdef"].concat()
    }

    fn fake_slack_token() -> String {
        ["xox", "b-", "123456789-", "abcdefghij"].concat()
    }

    fn fake_github_token() -> String {
        ["gh", "p_", "XaBcDeFgHiJkLmNoPqRsTuVwXyZ12345"].concat()
    }

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
        assert_eq!(help.command, Command::Help(HelpTopic::General));

        let version = Invocation::parse(vec!["--version".into()]).unwrap();
        assert_eq!(version.command, Command::Version);
    }

    #[test]
    fn parses_command_help_topics() {
        assert_eq!(
            Invocation::parse(vec!["config".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Config)
        );
        assert_eq!(
            Invocation::parse(vec!["inspect".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Inspect)
        );
        assert_eq!(
            Invocation::parse(vec!["trace".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Trace)
        );
        assert_eq!(
            Invocation::parse(vec!["replay".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Replay)
        );
        assert_eq!(
            Invocation::parse(vec!["verify-runtime".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::VerifyRuntime)
        );
    }

    #[test]
    fn fixed_arity_commands_reject_extra_arguments() {
        let config_error = Invocation::parse(vec!["config".into(), "extra".into()]).unwrap_err();
        assert!(config_error.to_string().contains("usage: olyx config"));

        let inspect_error = Invocation::parse(vec!["inspect".into(), "extra".into()]).unwrap_err();
        assert!(inspect_error.to_string().contains("usage: olyx inspect"));
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

        let blank_key = Invocation {
            command: Command::Config,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: Some("  ".into()),
            dry_run: false,
        };
        assert!(require_api_key(&blank_key).is_err());
    }

    #[test]
    fn signed_envelope_reader_rejects_oversized_files() {
        let path = std::env::temp_dir().join(format!(
            "olyx-cli-oversized-envelope-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(
            &path,
            vec![b' '; (MAX_SIGNED_ENVELOPE_FILE_BYTES + 1) as usize],
        )
        .unwrap();

        let error = read_signed_envelope(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("exceeds"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn signed_envelope_reader_rejects_invalid_json() {
        let path = std::env::temp_dir().join(format!(
            "olyx-cli-invalid-envelope-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "not-json").unwrap();

        let error = read_signed_envelope(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("invalid signed envelope JSON"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn dry_run_returns_request_shape() {
        let olyx_key = fake_olyx_key();
        let openai_key = fake_openai_key();
        let slack_token = fake_slack_token();
        let body = json!({
            "trace_id": "trace_123",
            "api_key": olyx_key,
            "authorization": format!("Bearer {openai_key}"),
            "notes": slack_token
        });
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
        assert_eq!(response["body"]["trace_id"], "trace_123");
        assert_eq!(response["body"]["api_key"], REDACTED);
        assert_eq!(response["body"]["authorization"], REDACTED);
        assert_eq!(response["body"]["notes"], "xoxb-[REDACTED]");
    }

    #[tokio::test]
    async fn config_dry_run_returns_get_shape_without_body() {
        let response = request_json(
            "GET",
            "https://app.olyxai.io",
            "/api/v1/sdk/config",
            None,
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(response["dry_run"], true);
        assert_eq!(response["method"], "GET");
        assert_eq!(response["url"], "https://app.olyxai.io/api/v1/sdk/config");
        assert_eq!(response["api_key_present"], false);
        assert_eq!(response["body"], Value::Null);
    }

    #[test]
    fn formats_public_api_error_shape() {
        let message = format_api_error(&json!({
            "error": "Unauthorized",
            "code": "unauthorized",
            "request_id": "req_123"
        }));

        assert_eq!(message, "Unauthorized (unauthorized, request_id=req_123)");
    }

    #[test]
    fn api_error_redacts_secret_values() {
        let openai_key = fake_openai_key();
        let olyx_key = fake_olyx_key();
        let message = format_api_error(&json!({
            "error": format!("upstream rejected Bearer {openai_key} and {olyx_key}"),
            "code": "unauthorized"
        }));

        assert!(!message.contains(&openai_key));
        assert!(!message.contains(&olyx_key));
        assert!(message.contains("Bearer [REDACTED]"));
        assert!(message.contains("ak_[REDACTED]"));
    }

    #[test]
    fn sanitizes_sensitive_json_keys_and_token_like_strings() {
        let olyx_key = fake_olyx_key();
        let openai_key = fake_openai_key();
        let github_token = fake_github_token();
        let value = json!({
            "api_key": olyx_key,
            "nested": {
                "authorization": format!("Bearer {openai_key}"),
                "note": format!("Use token {github_token} for auth.")
            }
        });

        let sanitized = sanitize_json_for_output(&value);
        assert_eq!(sanitized["api_key"], REDACTED);
        assert_eq!(sanitized["nested"]["authorization"], REDACTED);
        assert_eq!(
            sanitized["nested"]["note"],
            "Use token ghp_[REDACTED] for auth."
        );
    }

    #[test]
    fn cli_error_message_redacts_token_like_values() {
        let olyx_key = fake_olyx_key();
        let error = CliError::message(format!("request failed for {olyx_key}"));

        assert!(!error.to_string().contains(&olyx_key));
        assert!(error.to_string().contains("ak_[REDACTED]"));
    }

    #[test]
    fn help_output_snapshots_are_stable() {
        assert_eq!(help_text(HelpTopic::General), GENERAL_HELP);
        assert_eq!(help_text(HelpTopic::Inspect), INSPECT_HELP);
        assert_eq!(help_text(HelpTopic::Config), CONFIG_HELP);
        assert_eq!(help_text(HelpTopic::Trace), TRACE_HELP);
        assert_eq!(help_text(HelpTopic::Replay), REPLAY_HELP);
        assert_eq!(help_text(HelpTopic::VerifyRuntime), VERIFY_RUNTIME_HELP);
    }

}
