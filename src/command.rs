use crate::error::CliError;
use clap::{CommandFactory, Parser, Subcommand};
use std::net::IpAddr;

#[derive(Debug, Parser)]
#[command(name = "oy", version, about = "Route, replay, and debug model requests", long_about = None)]
struct Arguments {
    /// Configuration file; provider credentials stay in environment variables
    #[arg(long, global = true, env = "OLYX_CONFIG", default_value = "olyx.json", value_name = "FILE", value_parser = nonempty)]
    config: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug)]
pub(crate) struct Invocation {
    pub config: String,
    pub command: Command,
}
#[derive(Debug)]
pub(crate) enum ParsedInvocation {
    Run(Invocation),
    Display(String),
}

#[derive(Debug, PartialEq, Subcommand)]
pub(crate) enum Command {
    /// Create configuration and a sample request without overwriting files
    Init {
        #[arg(long, value_parser = ["openai", "huggingface"], conflicts_with = "plugin")]
        provider: Option<String>,
        #[arg(long, value_parser = nonempty, required_if_eq("provider", "huggingface"))]
        model: Option<String>,
        #[arg(long, default_value = "fast", value_parser = nonempty)]
        alias: String,
        #[arg(long, value_parser = nonempty, conflicts_with = "plugin")]
        base_url: Option<String>,
        /// Generate a Python adapter scaffold using this absolute interpreter path
        #[arg(long, value_name = "PYTHON", value_parser = nonempty, requires = "model")]
        plugin: Option<String>,
    },
    /// Run an application endpoint (localhost by default; remote binding requires OLYX_SERVER_API_KEY)
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u16).range(1..))]
        max_concurrency: u16,
    },
    /// Send a JSON request, recording a local trace
    Route {
        #[arg(long, value_parser = nonempty)]
        model: Option<String>,
        #[arg(long, default_value = "-", value_name = "FILE|-", value_parser = nonempty)]
        input: String,
        /// Preview the request without calling a provider
        #[arg(long)]
        dry_run: bool,
    },
    /// Run a saved request again, preserving the original trace (live provider calls)
    Replay {
        #[arg(value_parser = trace_id)]
        trace_id: String,
        #[arg(long = "compare", value_delimiter = ',', value_parser = alias, conflicts_with = "force_model", value_name = "ALIAS1,ALIAS2")]
        models: Vec<String>,
        #[arg(long = "model", value_parser = nonempty)]
        force_model: Option<String>,
        /// Preview the replay without calling a provider
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspect one trace, or list recent traces, without provider credentials
    Debug {
        #[arg(value_parser = trace_id)]
        trace_id: Option<String>,
    },
    /// Validate a configuration file without network or storage changes
    Check {
        #[arg(value_parser = nonempty)]
        file: String,
    },
    /// Diagnose installation, configuration, credentials, and SQLite storage without inference calls
    Doctor {
        /// Also send unauthenticated HEAD requests to provider endpoints
        #[arg(long)]
        probe: bool,
    },
}

impl Invocation {
    pub(crate) fn parse(args: Vec<String>) -> Result<ParsedInvocation, CliError> {
        match Arguments::try_parse_from(std::iter::once("oy".to_owned()).chain(args)) {
            Ok(arguments) => match arguments.command {
                Some(command) => Ok(ParsedInvocation::Run(Self {
                    config: arguments.config,
                    command,
                })),
                None => Ok(ParsedInvocation::Display(
                    Arguments::command().render_help().to_string(),
                )),
            },
            Err(error)
                if matches!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) =>
            {
                Ok(ParsedInvocation::Display(error.to_string()))
            }
            Err(error) => Err(CliError::usage(
                error.to_string().trim_start_matches("error: "),
            )),
        }
    }
}
fn nonempty(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("value must not be empty".into())
    } else {
        Ok(value.into())
    }
}
fn alias(value: &str) -> Result<String, String> {
    nonempty(value.trim())
}
fn trace_id(value: &str) -> Result<String, String> {
    if value.len() > 128 {
        return Err("trace ID exceeds 128 bytes".into());
    }
    nonempty(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<ParsedInvocation, CliError> {
        Invocation::parse(args.iter().map(|s| (*s).into()).collect())
    }
    #[test]
    fn configuration_is_global_and_help_is_generated() {
        for args in [
            vec!["--config", "custom.json", "debug"],
            vec!["debug", "--config", "custom.json"],
        ] {
            let ParsedInvocation::Run(invocation) = parse(&args).unwrap() else {
                panic!("expected a command")
            };
            assert_eq!(invocation.config, "custom.json");
        }
        for args in [
            vec!["check", "--help"],
            vec!["help", "serve"],
            vec!["--version"],
            vec![],
        ] {
            assert!(matches!(
                parse(&args).unwrap(),
                ParsedInvocation::Display(_)
            ));
        }
    }
    #[test]
    fn validates_values_conflicts_and_command_specific_flags() {
        for args in [
            vec!["serve", "--port", "70000"],
            vec!["serve", "--max-concurrency", "0"],
            vec!["init", "--alias", ""],
            vec!["replay", "trace", "--compare", "fast,,quality"],
            vec![
                "replay",
                "trace",
                "--compare",
                "fast,quality",
                "--model",
                "fast",
            ],
            vec!["doctor", "--dry-run"],
            vec!["init", "--plugin", "/usr/bin/python3"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
        assert!(parse(&["route", "--input", "-", "--dry-run"]).is_ok());
        assert!(parse(&["replay", "trace", "--compare", "fast,quality"]).is_ok());
    }
}
