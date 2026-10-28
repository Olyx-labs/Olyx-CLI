#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum HelpTopic {
    General,
    Route,
    Replay,
    Debug,
    Check,
    Doctor,
    Inspect,
    Serve,
    Init,
}
pub(crate) fn help_text(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::Doctor => "oy doctor [--probe] [--config FILE]\nCheck local configuration, credential presence, and SQLite storage without inference calls. --probe additionally sends unauthenticated HEAD requests to provider base URLs (5-second timeout each); connectivity is not credential or model validation. Checks never display keys or apply retention.\n",
        HelpTopic::Init => "oy init [--provider NAME] [--model MODEL] [--alias ALIAS] [--base-url URL] [--config FILE]\nCreate local configuration and a request example without overwriting existing files. Provider keys are never written to configuration.\n",
        HelpTopic::Serve => "oy serve [--local] [--host IP] [--port PORT] [--max-concurrency N] [--config FILE]\n--local restricts binding to loopback. Serve /v1/chat/completions, /v1/embeddings, and /v1/models using local routing and SQLite traces. Defaults: 127.0.0.1:8787, 32 concurrent requests. Set OLYX_SERVER_API_KEY for bearer authentication; required on non-loopback addresses. /healthz and /readyz are unauthenticated.\n",
        HelpTopic::General => "Olyx CLI — standalone routing, replay, and debugging\n\nUsage:\n  oy init [--provider NAME] [--model MODEL]\n  oy serve [--local] [--host IP] [--port PORT] [--max-concurrency N]\n  oy route [--model ALIAS] [--input FILE|-]\n  oy replay TRACE_ID [--model ALIAS | --compare ALIAS1,ALIAS2]\n  oy debug [TRACE_ID]\n  oy check FILE\n  oy doctor [--probe]\n  oy inspect\n  oy help [init|serve|route|replay|debug|check|doctor|inspect]\n  oy --version\n\nOptions:\n  --config FILE  Local configuration (default: OLYX_CONFIG or ./olyx.json)\n  --dry-run      Preview route/replay without provider calls\n\nRouting, replay, debugging, and self-hosted serving require no Olyx account. serve --local stays loopback-only. Provider keys stay in your environment.\n",
        HelpTopic::Route => "oy route [--model ALIAS] [--input FILE|-] [--config FILE]\nRead a JSON request from a file or stdin; route and store its trace locally.\n",
        HelpTopic::Replay => "oy replay TRACE_ID [--model ALIAS | --compare ALIAS1,ALIAS2] [--config FILE]\nReplay locally with live provider calls; preserve the original trace.\nComparisons attempt every alias and print all results; partial failures exit nonzero.\n",
        HelpTopic::Debug => "oy debug [TRACE_ID] [--config FILE]\nInspect one local trace or list recent traces without provider credentials.\n",
        HelpTopic::Check => "oy check FILE\nValidate local configuration without provider credentials.\n",
        HelpTopic::Inspect => "oy inspect [--config FILE]\nShow the local configuration path, file availability, storage mode, and CLI version. No network requests.\n",
    }
}
