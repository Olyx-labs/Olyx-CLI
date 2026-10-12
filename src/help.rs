#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum HelpTopic {
    General,
    Route,
    Debug,
    Inspect,
    Login,
    Config,
    Trace,
    Complete,
    Replay,
}

pub(crate) fn help_text(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => GENERAL_HELP,
        HelpTopic::Route => ROUTE_HELP,
        HelpTopic::Debug => DEBUG_HELP,
        HelpTopic::Inspect => INSPECT_HELP,
        HelpTopic::Login => LOGIN_HELP,
        HelpTopic::Config => CONFIG_HELP,
        HelpTopic::Trace => TRACE_HELP,
        HelpTopic::Complete => COMPLETE_HELP,
        HelpTopic::Replay => REPLAY_HELP,
    }
}

pub(crate) const GENERAL_HELP: &str = r#"olyx CLI

Usage:
  olyx route [--model alias] [--input request.json|-] --config olyx.json
  olyx replay <trace_id> --config olyx.json [--compare aliases | --model alias]
  olyx debug [trace_id] --config olyx.json
  olyx check olyx.json
  olyx help
  olyx help <inspect|login|config|trace|complete|replay>
  olyx --version
  olyx login [--base-url URL]
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]
  olyx complete <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]
  olyx replay <trace_id> [--compare model1,model2 | --model model] [--wait] [--base-url URL] [--api-key KEY] [--dry-run]

Global Options:
  --config FILE      Local routing/replay/debug configuration (or OLYX_CONFIG).
  --base-url URL     Olyx API base URL. Defaults to https://app.olyxai.io
  --api-key KEY      Project-scoped Olyx API key. Prefer OLYX_API_KEY in production;
                     passing a key via --api-key exposes it in the process table.
  --dry-run          Print the request shape without sending it.
  -h, --help         Print this help text.
  -V, --version      Print the CLI version.

Environment:
  OLYX_BASE_URL    Defaults to https://app.olyxai.io
  OLYX_API_KEY     Project-scoped Olyx API key
"#;

pub(crate) const INSPECT_HELP: &str = r#"olyx inspect

Usage:
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]

Print local CLI environment status without making network requests.
"#;

pub(crate) const LOGIN_HELP: &str = r#"olyx login

Usage:
  olyx login [--base-url URL]

Open the Olyx dashboard login page in your browser. This command does not read
or print API keys.
"#;

pub(crate) const CONFIG_HELP: &str = r#"olyx config

Usage:
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]

Fetch the project configuration contract from Olyx, including distribution,
guardrails, and replay mode.
"#;

pub(crate) const TRACE_HELP: &str = r#"olyx trace

Usage:
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]

Create a trace envelope in Olyx Cloud using metadata only or an opaque local
reference.
"#;

pub(crate) const COMPLETE_HELP: &str = r#"olyx complete

Usage:
  olyx complete <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]

Mark a trace as complete. Call this once after the model work for a trace
finishes. The server computes cost and latency summaries from the recorded steps.
"#;

pub(crate) const REPLAY_HELP: &str = r#"olyx replay

Usage:
  olyx replay <trace_id> [--compare model1,model2 | --model model] [--wait] [--base-url URL] [--api-key KEY] [--dry-run]

Replay locally or queue work on Cloud. --wait polls Cloud jobs for up to 120 seconds.
Use --config FILE for local replay. Local comparison returns synchronously
and uses your configured provider keys. Without --config it uses Olyx Cloud.
"#;

pub(crate) const ROUTE_HELP: &str = "olyx route [--model alias] [--input request.json|-] --config olyx.json\nRoutes a JSON chat request locally, records a trace, and uses configured fallback providers.\n";
pub(crate) const DEBUG_HELP: &str = "olyx debug [trace_id] --config olyx.json\nShows local trace inputs, outputs, model, latency, and cost. With no id, lists recent traces.\n";
