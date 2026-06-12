#![forbid(unsafe_code)]

mod command;
mod error;
mod help;
mod http;
mod redact;

use command::Command;
use error::CliError;
use help::help_text;
use http::{api_key_present, normalize_base_url, request_json};
use redact::sanitize_json_for_output;
use serde_json::{json, Value};
use std::{env, process};

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
        Command::Config | Command::Trace { .. } | Command::Complete { .. } | Command::Replay { .. }
    ) {
        require_api_key(&invocation)?;
    }

    match invocation.command {
        Command::Config => {
            let response = request_json(
                "GET",
                &invocation.base_url,
                "/api/v1/config",
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
        Command::Complete { trace_id } => {
            let path = format!("/api/v1/traces/{trace_id}/complete");
            let response = request_json(
                "PATCH",
                &invocation.base_url,
                &path,
                invocation.api_key.as_deref(),
                None,
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
        Command::Login => open_login(&invocation),
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

fn runtime_env_status() -> Value {
    json!({
        "license_path_present": env::var("OLYX_LICENSE_PATH").is_ok(),
        "policy_pack_path_present": env::var("OLYX_POLICY_PACK_PATH").is_ok(),
        "key_id_present": env::var("OLYX_POLICY_KEY_ID").is_ok(),
        "public_key_present": env::var("OLYX_POLICY_PUBLIC_KEY_B64").is_ok()
            || env::var("OLYX_POLICY_PUBLIC_KEY_PATH").is_ok(),
    })
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
                        Command::Help(help::HelpTopic::General),
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

fn require_api_key(invocation: &Invocation) -> Result<(), CliError> {
    if api_key_present(invocation.api_key.as_deref()) || invocation.dry_run {
        Ok(())
    } else {
        Err(CliError::usage(
            "OLYX_API_KEY is required; export it or pass --api-key",
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

fn open_login(invocation: &Invocation) -> Result<(), CliError> {
    let url = login_url(&invocation.base_url);

    if invocation.dry_run {
        return print_json(&json!({
            "dry_run": true,
            "command": "login",
            "url": url
        }));
    }

    println!("Opening {url}");
    if !browser_open(&url) {
        println!("Could not open a browser automatically. Copy this URL: {url}");
    }
    Ok(())
}

fn login_url(base_url: &str) -> String {
    format!("{}/login", base_url.trim_end_matches('/'))
}

fn browser_open(url: &str) -> bool {
    platform_browser_open(url)
}

#[cfg(target_os = "macos")]
fn platform_browser_open(url: &str) -> bool {
    process::Command::new("open")
        .arg(url)
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "windows")]
fn platform_browser_open(url: &str) -> bool {
    process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_browser_open(url: &str) -> bool {
    for program in ["xdg-open", "gio", "sensible-browser"] {
        let mut command = process::Command::new(program);
        if program == "gio" {
            command.arg("open");
        }
        if command
            .arg(url)
            .status()
            .is_ok_and(|status| status.success())
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::help::HelpTopic;
    use crate::redact::{sanitize_json_for_output, sanitize_text_for_output, REDACTED};

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
    fn parses_login_without_api_key() {
        let invocation = Invocation::parse(vec![
            "--base-url".into(),
            "https://app.olyxai.io/".into(),
            "login".into(),
            "--dry-run".into(),
        ])
        .unwrap();

        assert_eq!(invocation.command, Command::Login);
        assert_eq!(invocation.base_url, "https://app.olyxai.io");
        assert!(invocation.api_key.is_none());
        assert!(invocation.dry_run);
        assert_eq!(
            login_url(&invocation.base_url),
            "https://app.olyxai.io/login"
        );
        assert!(require_api_key(&invocation).is_ok());
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
            Invocation::parse(vec!["login".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Login)
        );
        assert_eq!(
            Invocation::parse(vec!["trace".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Trace)
        );
        assert_eq!(
            Invocation::parse(vec!["complete".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Complete)
        );
        assert_eq!(
            Invocation::parse(vec!["replay".into(), "--help".into()])
                .unwrap()
                .command,
            Command::Help(HelpTopic::Replay)
        );
    }

    #[test]
    fn fixed_arity_commands_reject_extra_arguments() {
        let config_error = Invocation::parse(vec!["config".into(), "extra".into()]).unwrap_err();
        assert!(config_error.to_string().contains("usage: olyx config"));

        let inspect_error = Invocation::parse(vec!["inspect".into(), "extra".into()]).unwrap_err();
        assert!(inspect_error.to_string().contains("usage: olyx inspect"));

        let login_error = Invocation::parse(vec!["login".into(), "extra".into()]).unwrap_err();
        assert!(login_error.to_string().contains("usage: olyx login"));
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
    fn parses_complete() {
        let invocation = Invocation::parse(vec!["complete".into(), "trace_abc123".into()]).unwrap();
        assert_eq!(
            invocation.command,
            Command::Complete {
                trace_id: "trace_abc123".into()
            }
        );
    }

    #[test]
    fn complete_rejects_oversized_trace_id() {
        let long_id = "a".repeat(129);
        let error = Invocation::parse(vec!["complete".into(), long_id]).unwrap_err();
        assert!(error.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn replay_rejects_oversized_trace_id() {
        let long_id = "a".repeat(129);
        let error = Invocation::parse(vec!["replay".into(), long_id]).unwrap_err();
        assert!(error.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn trace_rejects_oversized_local_ref() {
        let long_ref = "a".repeat(513);
        let error =
            Invocation::parse(vec!["trace".into(), "--local-ref".into(), long_ref]).unwrap_err();
        assert!(error.to_string().contains("exceeds maximum"));
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
            "/api/v1/config",
            None,
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(response["dry_run"], true);
        assert_eq!(response["method"], "GET");
        assert_eq!(response["url"], "https://app.olyxai.io/api/v1/config");
        assert_eq!(response["api_key_present"], false);
        assert_eq!(response["body"], Value::Null);
    }

    #[test]
    fn normalize_base_url_blocks_metadata_endpoints() {
        use crate::http::normalize_base_url;
        for blocked in &[
            "https://169.254.169.254",
            "https://169.254.169.254/latest/meta-data/",
            "https://100.100.100.200",
            "https://metadata.google.internal",
            "https://169.254.0.2",
        ] {
            assert!(
                normalize_base_url(blocked).is_err(),
                "{blocked} should be blocked"
            );
        }
        assert!(normalize_base_url("https://app.olyxai.io").is_ok());
        assert!(normalize_base_url("http://localhost:8080").is_ok());
    }

    #[test]
    fn sanitize_json_does_not_overflow_on_deep_nesting() {
        use crate::redact::sanitize_json_for_output;
        let mut value = json!("leaf");
        for _ in 0..100 {
            value = json!({ "nested": value });
        }
        let sanitized = sanitize_json_for_output(&value);
        assert!(sanitized.is_object());
    }

    #[test]
    fn api_error_redacts_secret_values() {
        let openai_key = fake_openai_key();
        let olyx_key = fake_olyx_key();
        let error = CliError::message(format!(
            "upstream rejected Bearer {openai_key} and {olyx_key}"
        ));

        assert!(!error.to_string().contains(&openai_key));
        assert!(!error.to_string().contains(&olyx_key));
        assert!(error.to_string().contains("Bearer [REDACTED]"));
        assert!(error.to_string().contains("ak_[REDACTED]"));
    }

    #[test]
    fn bearer_redaction_is_case_insensitive() {
        let openai_key = fake_openai_key();
        let lower = sanitize_text_for_output(&format!("bearer {openai_key}"));
        let upper = sanitize_text_for_output(&format!("BEARER {openai_key}"));
        assert!(
            !lower.contains(&openai_key),
            "lowercase bearer not redacted"
        );
        assert!(
            !upper.contains(&openai_key),
            "uppercase bearer not redacted"
        );
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
        use crate::help::{
            COMPLETE_HELP, CONFIG_HELP, GENERAL_HELP, INSPECT_HELP, LOGIN_HELP, REPLAY_HELP,
            TRACE_HELP,
        };
        assert_eq!(help_text(HelpTopic::General), GENERAL_HELP);
        assert_eq!(help_text(HelpTopic::Inspect), INSPECT_HELP);
        assert_eq!(help_text(HelpTopic::Login), LOGIN_HELP);
        assert_eq!(help_text(HelpTopic::Config), CONFIG_HELP);
        assert_eq!(help_text(HelpTopic::Trace), TRACE_HELP);
        assert_eq!(help_text(HelpTopic::Complete), COMPLETE_HELP);
        assert_eq!(help_text(HelpTopic::Replay), REPLAY_HELP);
    }
}
