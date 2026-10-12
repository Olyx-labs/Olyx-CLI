# Olyx CLI

Apache-2.0 command-line routing, replay, and debugging for AI model requests.
The public repository builds on its own: no private repositories, enterprise
licenses, Olyx account, or Olyx Cloud connection are required for local commands.

## Local workflow

```sh
cargo build --locked --release --bin olyx
export OPENAI_API_KEY=your-provider-key
./target/release/olyx check examples/olyx.json
./target/release/olyx route --config examples/olyx.json --input examples/request.json
./target/release/olyx debug --config examples/olyx.json
./target/release/olyx debug TRACE_ID --config examples/olyx.json
./target/release/olyx replay TRACE_ID --config examples/olyx.json --compare fast,quality
```

Install the built binary on your PATH to use `olyx` directly. JSON output from
`route` contains `trace_id` and the provider response. Use that trace ID for
`debug` and `replay`. `OLYX_CONFIG` can replace `--config` on these commands.

`route` reads an OpenAI-compatible chat-completions request from `--input FILE`
or stdin (`--input -`, the default). `--model ALIAS` overrides its `model` field.
A model alias maps to an ordered list of providers. The CLI tries the first,
then falls back on connection failures, HTTP 429, or provider 5xx responses.
Authentication and invalid-request errors stop the route. Provider credentials
come from the configured environment variable, never from the configuration file.

`debug TRACE_ID` displays local recorded inputs/outputs, attempted models,
fallback errors, latency, and estimated cost. Without an ID it lists the 100
most recent traces. It works without provider credentials and without network access.

`replay TRACE_ID --config FILE` re-executes the recorded successful model steps
against their original routes. `--model ALIAS` selects one alternative;
`--compare ALIAS1,ALIAS2` runs a comparison. It creates new traces and preserves
the original. Results come from live model calls; provider failure returns an
error rather than a simulated answer. Local replay is synchronous.

This first local runtime supports non-streaming OpenAI-compatible
`/chat/completions` providers, including compatible private model endpoints.
It returns tool calls but does not execute tools or reconstruct a whole agent
workflow. Replay compares recorded model requests, not external tool side effects.
Native Anthropic/Gemini protocols and streaming are not implemented here.

## Configuration and storage

See [examples/olyx.json](examples/olyx.json). URLs require HTTPS except for
loopback endpoints. List additional providers under an alias to configure
fallbacks. `check FILE` validates configuration without requiring credentials.

Set `input_per_million` and `output_per_million` to your provider's rates before
using cost tracking or `monthly_budget_usd`. The example deliberately uses zero
prices and disables the spend cap; it does not claim current provider pricing.
Costs are estimates using configured prices and provider-reported token usage.
The monthly budget uses conservative reservations shared across CLI processes.
Transport failures retain reservations because upstream charges may be ambiguous.
It is not a substitute for provider billing limits.

The SQLite database path is relative to the configuration file. Trace retention
is configurable and enforced when a local runtime command opens the database.
A new trace directory is owner-only on Unix; payloads are stored locally, without
cloud sync. Keep the database private. CLI output masks recognizable credentials.
Routing and replay preserve prompt and response content. Request-size and
output-token limits validate requests; they do not scan or redact content.

Olyx Cloud's 10,000 recorded traces and 50 replay-call allowances do not apply to
local execution. You pay your provider and supply your own storage and compute.

## Existing Cloud commands

```sh
export OLYX_API_KEY=ak_...
olyx inspect
olyx config
olyx trace --metadata '{"service":"checkout"}'
olyx complete TRACE_ID
olyx replay TRACE_ID --compare MODEL1,MODEL2 --wait
```

Without `--config` or `OLYX_CONFIG`, replay uses the existing Cloud API.
`--wait` polls queued Cloud jobs for up to 120 seconds. `--base-url` and
`OLYX_BASE_URL` select a Cloud or compatible backend. `--dry-run` displays a
request without calling a provider or creating a trace.

A Cloud `trace` command creates an envelope; it does not intercept subsequent
provider calls. Record model work through Olyx's proxy/API to populate that trace.

## Source boundary

- `src/`: CLI parsing, Cloud transport, and local command orchestration.
- `runtime/`: local routing, SQLite traces, replay/comparison, and debug data.

All of this repository is Apache-2.0. It contains no commercial activation,
signing keys, fleet administration, hosted judges, SSO, or enterprise license
implementation. Managed hosting and organizational controls remain separate.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo deny check --config deny.toml advisories licenses bans sources
```

Supported release targets: Linux x86_64/arm64, macOS Intel/Apple Silicon, and
Windows x86_64. SQLite is bundled into the binary; no database server is required.
See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
