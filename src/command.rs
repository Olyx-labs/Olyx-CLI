use serde_json::Value;

use crate::error::CliError;
use crate::help::HelpTopic;

// Upper bounds on user-supplied string inputs. Real values are much smaller;
// these limits exist to prevent oversized inputs from consuming memory or
// appearing verbatim in logs and error messages.
const MAX_TRACE_ID_BYTES: usize = 128;
const MAX_LOCAL_REF_BYTES: usize = 512;
const MAX_METADATA_JSON_BYTES: usize = 65_536;

#[derive(Debug, PartialEq)]
pub(crate) enum Command {
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

impl Command {
    pub(crate) fn parse(args: Vec<String>) -> Result<Self, CliError> {
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

pub(crate) fn parse_help_topic(raw: &str) -> Result<HelpTopic, CliError> {
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

fn parse_help(args: Vec<String>) -> Result<Command, CliError> {
    match args.as_slice() {
        [_] => Ok(Command::Help(HelpTopic::General)),
        [_, topic] => Ok(Command::Help(parse_help_topic(topic)?)),
        _ => Err(CliError::usage(
            "usage: olyx help [inspect|config|trace|replay|verify-runtime]",
        )),
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
                if raw.len() > MAX_METADATA_JSON_BYTES {
                    return Err(CliError::usage(format!(
                        "--metadata JSON exceeds maximum of {MAX_METADATA_JSON_BYTES} bytes"
                    )));
                }
                metadata = Some(
                    serde_json::from_str(raw)
                        .map_err(|e| CliError::usage(format!("invalid metadata JSON: {e}")))?,
                );
            }
            "--local-ref" => {
                i += 1;
                let value = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| CliError::usage("--local-ref requires a value"))?;
                if value.len() > MAX_LOCAL_REF_BYTES {
                    return Err(CliError::usage(format!(
                        "--local-ref value exceeds maximum of {MAX_LOCAL_REF_BYTES} bytes"
                    )));
                }
                local_ref = Some(value);
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

    let trace_id = args[1].clone();
    if trace_id.len() > MAX_TRACE_ID_BYTES {
        return Err(CliError::usage(format!(
            "trace_id exceeds maximum of {MAX_TRACE_ID_BYTES} bytes"
        )));
    }

    Ok(Command::Replay { trace_id })
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

fn redact_argument(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() > 80 {
        format!("{}...", trimmed.chars().take(80).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
