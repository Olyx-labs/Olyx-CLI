use serde_json::Value;

use crate::error::CliError;
use crate::help::HelpTopic;

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
    Complete {
        trace_id: String,
    },
    Replay {
        trace_id: String,
    },
    Login,
    Inspect,
    Help(HelpTopic),
    Version,
}

impl Command {
    pub(crate) fn parse(args: Vec<String>) -> Result<Self, CliError> {
        match args.first().map(String::as_str) {
            None => Ok(Self::Help(HelpTopic::General)),
            Some("help") => parse_help(args),
            Some("config") => parse_config(args),
            Some("inspect") => parse_fixed_arity_help(args, HelpTopic::Inspect, Self::Inspect),
            Some("login") => parse_login(args),
            Some("trace") => parse_trace(args),
            Some("complete") => parse_complete(args),
            Some("replay") => parse_replay(args),
            Some(other) => Err(CliError::usage(format!(
                "unknown command: {}",
                redact_argument(other)
            ))),
        }
    }
}

fn parse_config(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 1 {
        return Ok(Command::Config);
    }

    if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help(HelpTopic::Config));
    }

    Err(CliError::usage(
        "usage: olyx config [--base-url URL] [--api-key KEY] [--dry-run]",
    ))
}

pub(crate) fn parse_help_topic(raw: &str) -> Result<HelpTopic, CliError> {
    match raw {
        "inspect" => Ok(HelpTopic::Inspect),
        "login" => Ok(HelpTopic::Login),
        "config" => Ok(HelpTopic::Config),
        "trace" => Ok(HelpTopic::Trace),
        "complete" => Ok(HelpTopic::Complete),
        "replay" => Ok(HelpTopic::Replay),
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
            "usage: olyx help [inspect|login|config|trace|complete|replay]",
        )),
    }
}

fn parse_fixed_arity_help(
    args: Vec<String>,
    help_topic: HelpTopic,
    command: Command,
) -> Result<Command, CliError> {
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

fn parse_login(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 1 {
        Ok(Command::Login)
    } else if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        Ok(Command::Help(HelpTopic::Login))
    } else {
        Err(CliError::usage(
            "usage: olyx login [--base-url URL] [--dry-run]",
        ))
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

fn parse_complete(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2
        && args
            .get(1)
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help(HelpTopic::Complete));
    }

    if args.len() != 2 {
        return Err(CliError::usage("usage: olyx complete <trace_id>"));
    }

    let trace_id = args[1].clone();
    if trace_id.len() > MAX_TRACE_ID_BYTES {
        return Err(CliError::usage(format!(
            "trace_id exceeds maximum of {MAX_TRACE_ID_BYTES} bytes"
        )));
    }

    Ok(Command::Complete { trace_id })
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

fn redact_argument(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() > 80 {
        format!("{}...", trimmed.chars().take(80).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
