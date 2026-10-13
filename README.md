# Olyx CLI

Apache-2.0 command-line routing, replay, and debugging for AI model requests.
The public repository builds on its own: no private repositories, enterprise
licenses, Olyx account, or Olyx Cloud connection are required for local commands.

## Install

Download the archive for your platform from [GitHub Releases](https://github.com/Olyx-labs/Olyx-CLI/releases).
Each archive contains the binary, examples, LICENSE, NOTICE, and README files.
Verify the adjacent checksum and Sigstore bundle before running it:

```sh
asset=olyx-v0.2.0-aarch64-apple-darwin.tar.gz
shasum -a 256 -c "$asset.sha256"
cosign verify-blob --bundle "$asset.bundle" \
  --certificate-identity 'https://github.com/Olyx-labs/Olyx-CLI/.github/workflows/release.yml@refs/tags/v0.2.0' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com "$asset"
tar -xzf "$asset"
cd olyx-v0.2.0-aarch64-apple-darwin
./olyx check examples/olyx.json
```

Use the archive matching your OS and CPU. The example names the prepared 0.2.0
release; use an available release from GitHub Releases until it is published.
Windows users can verify with `Get-FileHash`, extract the `.zip`, and put
`olyx.exe` on PATH. Source builds require Rust 1.88 or newer and a C compiler
for bundled SQLite.

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

The runtime supports OpenAI-compatible chat completions, native Anthropic Messages,
and native Gemini GenerateContent. Set `protocol` to `openai` (default),
`anthropic`, or `gemini` on each provider. Use base URLs ending in `/v1` for
OpenAI and Anthropic, and `/v1beta` for Gemini.

Add `"stream": true` to a request to receive flushed NDJSON snapshots during
routing, followed by a `completed` event containing the trace ID and full response.
Each snapshot contains the cumulative response rather than only the latest delta.
Debug stores the completed response; replay and comparison consume recorded
streaming requests and return their accumulated results. An interrupted stream
fails the trace and stops the route; it does not start another provider after
partial output. Failed steps are excluded from replay.

Common text messages, system instructions, temperature, top-p, stop sequences,
and output token controls are translated for native providers. For tools, images,
reasoning options, or other native features, use an envelope with `model`, optional
`stream` and `max_tokens`, plus `native_request` containing the provider's request
body. The configured model, streaming mode, and output token limit take precedence.
Native responses retain `native_response`, including tool calls and thought
signatures. Return those native blocks in subsequent native requests when required
by the provider. Unsupported common fields return an error rather than being
silently dropped. Compare native envelopes only across routes using the same
protocol; use common text messages for comparisons across providers. Tool calls are returned; the CLI does not execute tools.

```json
{
  "model": "claude",
  "stream": true,
  "native_request": {
    "messages": [{"role": "user", "content": "Hello"}]
  }
}
```

## Configuration and storage

See [Database and storage setup](docs/storage.md) for durable/in-memory options,
retention, permissions, backup, and restore.

See [examples/olyx.json](examples/olyx.json). URLs require HTTPS except for
loopback endpoints. List additional providers under an alias to configure
fallbacks. `check FILE` validates configuration without requiring credentials.

The SQLite database path is relative to the configuration file. Trace retention
is configurable and enforced when a local runtime command opens the database.
A new trace directory is owner-only on Unix; payloads are stored locally, without
cloud sync. Keep the database private. CLI output masks recognizable credentials.
Routing and replay preserve prompt and response content. Request-size and
output-token limits validate requests; they do not scan or redact content.

Local execution uses your provider credentials, storage, and compute.

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
python3 scripts/smoke-local.py target/release/olyx
```

Supported release targets: Linux x86_64/arm64, macOS Intel/Apple Silicon, and
Windows x86_64. SQLite is bundled into the binary; no database server is required.
See [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and
[RELEASING.md](RELEASING.md). The prepared 0.2.0 changes are in [CHANGELOG.md](CHANGELOG.md).
