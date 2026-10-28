use crate::{error::CliError, help::HelpTopic};
const MAX_TRACE_ID_BYTES: usize = 128;
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
    Replay {
        trace_id: String,
        models: Vec<String>,
        force_model: Option<String>,
    },
    Serve {
        local: bool,
        listen: std::net::SocketAddr,
        max_concurrency: usize,
    },
    Init {
        provider: Option<String>,
        model: Option<String>,
        alias: String,
        base_url: Option<String>,
    },
    Doctor {
        probe: bool,
    },
    Inspect,
    Help(HelpTopic),
    Version,
}
impl Command {
    pub(crate) fn parse(args: Vec<String>) -> Result<Self, CliError> {
        match args.first().map(String::as_str) {
            None => Ok(Self::Help(HelpTopic::General)),
            Some("serve") => parse_serve(args),
            Some("init") => parse_init(args),
            Some("route") => parse_route(args),
            Some("replay") => parse_replay(args),
            Some("debug") => {
                if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
                    return Ok(Self::Help(HelpTopic::Debug));
                }
                if args.len() > 2 || args.get(1).is_some_and(|s| s.starts_with('-')) {
                    return Err(CliError::usage("usage: oy debug [trace_id]"));
                }
                Ok(Self::Debug {
                    trace_id: args.get(1).cloned(),
                })
            }
            Some("check") if args.len() == 2 => Ok(Self::Check {
                config: args[1].clone(),
            }),
            Some("doctor") => match args.get(1).map(String::as_str) {
                None if args.len() == 1 => Ok(Self::Doctor { probe: false }),
                Some("--probe") if args.len() == 2 => Ok(Self::Doctor { probe: true }),
                Some("--help" | "-h") if args.len() == 2 => Ok(Self::Help(HelpTopic::Doctor)),
                _ => Err(CliError::usage(
                    "usage: oy doctor [--probe] [--config FILE]",
                )),
            },
            Some("inspect") if args.len() == 1 => Ok(Self::Inspect),
            Some("help") => {
                if args.len() > 2 {
                    return Err(CliError::usage(
                        "usage: oy help [init|serve|route|replay|debug|check|doctor|inspect]",
                    ));
                }
                let topic = match args.get(1).map(String::as_str) {
                    None => HelpTopic::General,
                    Some("serve") => HelpTopic::Serve,
                    Some("init") => HelpTopic::Init,
                    Some("route") => HelpTopic::Route,
                    Some("replay") => HelpTopic::Replay,
                    Some("debug") => HelpTopic::Debug,
                    Some("check") => HelpTopic::Check,
                    Some("inspect") => HelpTopic::Inspect,
                    Some("doctor") => HelpTopic::Doctor,
                    _ => return Err(CliError::usage("unknown help topic")),
                };
                Ok(Self::Help(topic))
            }
            _ => Err(CliError::usage("unknown command or arguments; run oy help")),
        }
    }
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
            CliError::usage("usage: oy replay <trace_id> [--compare model1,model2] [--model model]")
        })?;
    if trace_id.len() > MAX_TRACE_ID_BYTES {
        return Err(CliError::usage(format!(
            "trace_id exceeds maximum of {MAX_TRACE_ID_BYTES} bytes"
        )));
    }
    let mut models = Vec::new();
    let mut force_model = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
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
    })
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
                    "usage: oy route [--model alias] [--input request.json|-] --config olyx.json",
                ))
            }
        }
        index += 1;
    }
    Ok(Command::Route { model, input })
}

fn parse_serve(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        return Ok(Command::Help(HelpTopic::Serve));
    }
    let mut host = "127.0.0.1".parse::<std::net::IpAddr>().unwrap();
    let mut port = 8787;
    let mut max_concurrency = 32;
    let mut local = false;
    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        i += 1;
        if flag == "--local" {
            local = true;
            continue;
        }
        let value = args
            .get(i)
            .ok_or_else(|| CliError::usage("serve flags require a value"))?;
        match flag.as_str() {
            "--host" => {
                host = value
                    .parse()
                    .map_err(|_| CliError::usage("--host must be an IP address"))?
            }
            "--port" => {
                port = value
                    .parse()
                    .map_err(|_| CliError::usage("--port must be 0..65535"))?
            }
            "--max-concurrency" => {
                max_concurrency = value
                    .parse()
                    .ok()
                    .filter(|n| *n > 0 && *n <= 65535)
                    .ok_or_else(|| CliError::usage("--max-concurrency must be 1..65535"))?
            }
            _ => return Err(CliError::usage("unknown serve flag")),
        }
        i += 1;
    }
    if local && !host.is_loopback() {
        return Err(CliError::usage(
            "serve --local only supports a loopback --host",
        ));
    }
    Ok(Command::Serve {
        local,
        listen: std::net::SocketAddr::new(host, port),
        max_concurrency,
    })
}
fn parse_init(args: Vec<String>) -> Result<Command, CliError> {
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        return Ok(Command::Help(HelpTopic::Init));
    }
    let (mut provider, mut model, mut base_url) = (None, None, None);
    let mut alias = "fast".to_owned();
    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        i += 1;
        let value = args
            .get(i)
            .filter(|s| !s.is_empty() && !s.starts_with('-'))
            .ok_or_else(|| CliError::usage("init flags require a value"))?
            .clone();
        match flag.as_str() {
            "--provider" => provider = Some(value),
            "--model" => model = Some(value),
            "--alias" => alias = value,
            "--base-url" => base_url = Some(value),
            _ => return Err(CliError::usage("unknown init flag")),
        }
        i += 1;
    }
    Ok(Command::Init {
        provider,
        model,
        alias,
        base_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Command, CliError> {
        Command::parse(args.iter().map(|arg| (*arg).to_owned()).collect())
    }
    #[test]
    fn route_accepts_stdin_and_explicit_aliases() {
        assert_eq!(
            parse(&["route"]).unwrap(),
            Command::Route {
                model: None,
                input: "-".into()
            }
        );
        assert_eq!(
            parse(&["route", "--model", "fast", "--input", "request.json"]).unwrap(),
            Command::Route {
                model: Some("fast".into()),
                input: "request.json".into()
            }
        );
    }
    #[test]
    fn replay_comparison_and_override_are_mutually_exclusive() {
        assert!(parse(&["replay", "trace_1", "--compare", "fast,quality"]).is_ok());
        assert!(parse(&["replay", "trace_1", "--model", "quality"]).is_ok());
        assert!(parse(&[
            "replay",
            "trace_1",
            "--model",
            "quality",
            "--compare",
            "fast,quality"
        ])
        .is_err());
    }
}
