#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum HelpTopic {
    General,
    Inspect,
    Config,
    Trace,
    Replay,
    VerifyRuntime,
}

pub(crate) fn help_text(topic: HelpTopic) -> &'static str {
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

pub(crate) const GENERAL_HELP: &str = r#"olyx CLI

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

pub(crate) const INSPECT_HELP: &str = r#"olyx inspect

Usage:
  olyx inspect [--base-url URL] [--api-key KEY] [--dry-run]

Print local CLI and private-runtime environment status without making network
requests.
"#;

pub(crate) const CONFIG_HELP: &str = r#"olyx config

Usage:
  olyx config [--base-url URL] [--api-key KEY] [--dry-run]

Fetch the project configuration contract from Olyx Cloud, including distribution,
guardrails, and replay mode.
"#;

pub(crate) const TRACE_HELP: &str = r#"olyx trace

Usage:
  olyx trace [--metadata JSON] [--local-ref REF] [--base-url URL] [--api-key KEY] [--dry-run]

Create a trace envelope in Olyx Cloud using metadata only or an opaque local
reference.
"#;

pub(crate) const REPLAY_HELP: &str = r#"olyx replay

Usage:
  olyx replay <trace_id> [--base-url URL] [--api-key KEY] [--dry-run]

Queue replay work for an existing trace id.
"#;

pub(crate) const VERIFY_RUNTIME_HELP: &str = r#"olyx verify-runtime

Usage:
  olyx verify-runtime --license PATH --policy PATH --key-id KID (--public-key-b64 KEY | --public-key-file PATH)

Verify the signed runtime license and policy-pack envelopes required to activate
the private compiled core.
"#;
