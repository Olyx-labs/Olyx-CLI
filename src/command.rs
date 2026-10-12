use serde_json::Value;

use crate::error::CliError;
use crate::help::HelpTopic;

const MAX_TRACE_ID_BYTES: usize = 128;
const MAX_LOCAL_REF_BYTES: usize = 512;
const MAX_METADATA_JSON_BYTES: usize = 65_536;

#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Route {
        model: Option<String>,
        input: String,
    },
    Debug {
        trace_id: Option<String>,
    },
    Check {
        config: String,
    },
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
        models: Vec<String>,
        force_model: Option<String>,
        wait: bool,
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
            Some("route") => parse_route(args),
            Some("debug") => {
                if args.len() == 2 && args[1] == "--help" {
                    return Ok(Self::Help(HelpTopic::Debug));
                }
                if args.len() > 2 {
                    return Err(CliError::usage(
                        "usage: olyx debug [trace_id] --config olyx.json",
                    ));
                }
                Ok(Self::Debug {
                    trace_id: args.get(1).cloned(),
                })
            }
            Some("check") => {
                if args.len() != 2 {
                    return Err(CliError::usage("usage: olyx check <config.json>"));
                }
                Ok(Self::Check {
                    config: args[1].clone(),
                })
            }
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
        "route" => Ok(HelpTopic::Route),
        "debug" => Ok(HelpTopic::Debug),
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

    let trace_id = args
        .get(1)
        .filter(|id| !id.starts_with('-'))
        .cloned()
        .ok_or_else(|| {
            CliError::usage(
                "usage: olyx replay <trace_id> [--compare model1,model2] [--model model] [--wait]",
            )
        })?;
    if trace_id.len() > MAX_TRACE_ID_BYTES {
        return Err(CliError::usage(format!(
            "trace_id exceeds maximum of {MAX_TRACE_ID_BYTES} bytes"
        )));
    }
    let mut models = Vec::new();
    let mut force_model = None;
    let mut wait = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--wait" => wait = true,
            "--compare" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| CliError::usage("--compare needs models"))?;
                models = value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                if models.is_empty() {
                    return Err(CliError::usage("--compare needs models"));
                }
            }
            "--model" => {
                index += 1;
                force_model = Some(
                    args.get(index)
                        .filter(|s| !s.is_empty() && !s.starts_with('-'))
                        .cloned()
                        .ok_or_else(|| CliError::usage("--model needs a model"))?,
                );
            }
            _ => return Err(CliError::usage("unknown replay flag")),
        }
        index += 1;
    }
    if force_model.is_some() && !models.is_empty() {
        return Err(CliError::usage(
            "--model and --compare are mutually exclusive",
        ));
    }
    Ok(Command::Replay {
        trace_id,
        models,
        force_model,
        wait,
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

fn parse_route(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Route));
    }
    let mut model = None;
    let mut input = "-".to_owned();
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| CliError::usage("route flags require a value"))?
            .clone();
        match flag {
            "--model" => model = Some(value),
            "--input" => input = value,
            _ => {
                return Err(CliError::usage(
                    "usage: olyx route [--model alias] [--input request.json|-] --config olyx.json",
                ))
            }
        }
        index += 1;
    }
    Ok(Command::Route { model, input })
}

#[cfg(test)]
mod local_command_tests {
    use super::*;

    #[test]
    fn route_accepts_model_and_input() {
        assert_eq!(
            Command::parse(vec![
                "route".into(),
                "--model".into(),
                "fast".into(),
                "--input".into(),
                "request.json".into()
            ])
            .unwrap(),
            Command::Route {
                model: Some("fast".into()),
                input: "request.json".into()
            }
        );
    }

    #[test]
    fn route_defaults_to_stdin() {
        assert_eq!(
            Command::parse(vec!["route".into()]).unwrap(),
            Command::Route {
                model: None,
                input: "-".into()
            }
        );
    }

    #[test]
    fn comparison_and_model_override_are_mutually_exclusive() {
        assert!(Command::parse(vec![
            "replay".into(),
            "trace".into(),
            "--model".into(),
            "fast".into(),
            "--compare".into(),
            "a,b".into()
        ])
        .is_err());
    }

    #[test]
    fn comparison_and_wait_are_parsed() {
        assert_eq!(
            Command::parse(vec![
                "replay".into(),
                "trace".into(),
                "--compare".into(),
                "fast,quality".into(),
                "--wait".into()
            ])
            .unwrap(),
            Command::Replay {
                trace_id: "trace".into(),
                models: vec!["fast".into(), "quality".into()],
                force_model: None,
                wait: true
            }
        );
    }

    #[test]
    fn debug_can_list_or_inspect_one_trace() {
        assert_eq!(
            Command::parse(vec!["debug".into()]).unwrap(),
            Command::Debug { trace_id: None }
        );
        assert_eq!(
            Command::parse(vec!["debug".into(), "trace".into()]).unwrap(),
            Command::Debug {
                trace_id: Some("trace".into())
            }
        );
    }
}
