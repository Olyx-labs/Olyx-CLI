# Olyx CLI

`olyx` is the public control-plane CLI for Olyx. It manages trace lifecycle,
project configuration, and replay coordination against Olyx Cloud.

The complete outbound agent is assembled from three components:

| Component | Visibility | Role |
| :--- | :--- | :--- |
| **olyx-cli** (this repo) | Open source | Control plane — trace, complete, replay, config |
| **olyx-edge** | Private | Data plane — outbound proxy deployed in your infrastructure |
| **olyx-core** | Private | Cryptographic runtime — license and policy-pack verification |

`olyx-edge` is what your application's `base_url` points to. It applies
guardrails, rate limiting, and provider routing before forwarding requests to
the model provider. `olyx-core` verifies that the edge node is running a
signed, unexpired policy pack before it accepts traffic. The CLI manages the
trace records that both components write against.

- License: [Apache-2.0](LICENSE)
- Security: [SECURITY.md](SECURITY.md)
- Contributions: [CONTRIBUTING.md](CONTRIBUTING.md)

## Install

Download the archive for your platform from the latest GitHub Release. Verify
the checksum before installing.

### macOS Apple Silicon

```sh
version="v0.1.0"
asset="olyx-${version}-aarch64-apple-darwin.tar.gz"

curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}.sha256"
shasum -a 256 -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-aarch64-apple-darwin/olyx" /usr/local/bin/olyx
olyx --version
```

### macOS Intel

```sh
version="v0.1.0"
asset="olyx-${version}-x86_64-apple-darwin.tar.gz"

curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}.sha256"
shasum -a 256 -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-x86_64-apple-darwin/olyx" /usr/local/bin/olyx
olyx --version
```

### Linux x86_64

Linux builds are static. They do not require glibc or OpenSSL on the host.

```sh
version="v0.1.0"
asset="olyx-${version}-x86_64-unknown-linux-musl.tar.gz"

curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}.sha256"
sha256sum -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-x86_64-unknown-linux-musl/olyx" /usr/local/bin/olyx
olyx --version
```

### Linux arm64

```sh
version="v0.1.0"
asset="olyx-${version}-aarch64-unknown-linux-musl.tar.gz"

curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}.sha256"
sha256sum -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-aarch64-unknown-linux-musl/olyx" /usr/local/bin/olyx
olyx --version
```

### Windows x86_64

Run in PowerShell:

```powershell
$version = "v0.1.0"
$asset = "olyx-$version-x86_64-pc-windows-msvc.zip"

Invoke-WebRequest -Uri "https://github.com/olyxlabs/olyx-cli/releases/download/$version/$asset" -OutFile $asset
Invoke-WebRequest -Uri "https://github.com/olyxlabs/olyx-cli/releases/download/$version/$asset.sha256" -OutFile "$asset.sha256"

(Get-FileHash $asset -Algorithm SHA256).Hash.ToLower() + "  " + $asset |
  Compare-Object (Get-Content "$asset.sha256")

Expand-Archive $asset -DestinationPath .
Move-Item "olyx-$version-x86_64-pc-windows-msvc\olyx.exe" "$env:LOCALAPPDATA\Microsoft\WindowsApps\olyx.exe"
olyx --version
```

### Verify Provenance

Each release archive includes a Sigstore bundle.

```sh
cosign verify-blob \
  --bundle "${asset}.bundle" \
  --certificate-identity-regexp 'https://github.com/Olyx-labs/Olyx-CLI/.github/workflows/release.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "${asset}"
```

## Quickstart

```sh
export OLYX_API_KEY="ak_..."

olyx inspect
olyx config
olyx trace --metadata '{"environment":"production","service":"checkout","operation":"cart.create"}'
```

`OLYX_BASE_URL` defaults to `https://app.olyxai.io`.

## How It Fits Together

The CLI is a stateless thin client. `OLYX_BASE_URL` is the only knob — point
it at Olyx Cloud for SaaS, or at an `olyx-edge` node for enterprise. Everything
else is identical.

**Cloud**

```
OLYX_BASE_URL=https://app.olyxai.io

Your app  →  Olyx Cloud (/v1/chat/completions — model proxy)
olyx-cli  →  Olyx Cloud (/api/v1/* — trace, complete, replay, config)
```

**Enterprise (self-hosted edge)**

```
OLYX_BASE_URL=https://olyx-edge.internal

Your app  →  olyx-edge (/v1/chat/completions)
                ↓  guardrails, rate limiting, provider routing
                ↓  verified by olyx-core (signed license + policy pack)
             model provider

olyx-cli  →  olyx-edge (/api/v1/*)
                ↓  config, traces, complete, replay served locally
                ↓  no Olyx Cloud communication required
             (optional) Olyx Cloud sync via OLYX_CLOUD_SYNC_URL
```

Your application's `base_url` change is the same in both cases:

```python
client = ProviderAI(
    api_key=os.environ["PROVIDER_API_KEY"],
    base_url=os.environ["OLYX_BASE_URL"] + "/v1",
)
```

The provider key never leaves your environment. For enterprise deployments,
`olyx-edge` is fully air-gapped by default — set `OLYX_CLOUD_SYNC_URL` only
if you want trace events forwarded to Olyx Cloud as a secondary path.

## Commands

```sh
olyx help
olyx login
olyx inspect
olyx config
olyx config generate --language python
olyx trace --metadata JSON
olyx trace --local-ref REF
olyx complete TRACE_ID
olyx replay TRACE_ID
```

### `olyx login`

Opens the Olyx dashboard sign-in page in your browser.

```sh
olyx login
```

To print the URL without opening a browser:

```sh
olyx login --dry-run
```

### `olyx inspect`

Checks your local CLI configuration without making any network requests.

```sh
olyx inspect
```

Example output:

```json
{
  "base_url": "https://app.olyxai.io",
  "api_key_present": true,
  "dry_run": false,
  "secrets_boundary": "customer_environment"
}
```

### `olyx config`

Fetches project configuration from Olyx Cloud.

```sh
olyx config
```

Generate copy-paste setup for your application stack:

```sh
olyx config generate --language python
olyx config generate --language node --provider anthropic
olyx config generate --language curl
```

The generator reads the same `/api/v1/config` contract as `olyx config`, then
prints environment-variable based setup for the Olyx base URL. Provider API
keys stay in your application environment.

### `olyx trace`

Sends trace metadata to Olyx Cloud before the model call.

```sh
olyx trace --metadata '{
  "environment": "production",
  "service": "support-agent",
  "operation": "ticket.reply",
  "model": "gpt-4.1-mini"
}'
```

When using `olyx-edge`, the edge creates traces automatically as requests pass
through. Use `olyx trace` directly when you want explicit control over what
metadata Olyx Cloud receives, or when your application does not route through
an edge node.

### `olyx trace --local-ref`

Attach an opaque reference to a trace when the full request payload lives in
your own storage.

```sh
olyx trace \
  --local-ref "s3://acme-private-traces/prod/trace_01HYW8.json" \
  --metadata '{
    "environment": "production",
    "service": "claims-agent",
    "operation": "claim.review"
  }'
```

The reference should only resolve inside your environment. Do not use a public
URL unless you intentionally want that object publicly reachable.

### `olyx complete`

Marks a trace as complete. The server computes cost and latency summaries from
the recorded steps.

```sh
olyx complete trace_01HYW8A4Q9F7G9N2M3P4
```

When routing through `olyx-edge`, the edge sends completion automatically.
Call this directly when managing traces without an edge node.

### `olyx replay`

Requests replay coordination for a trace.

```sh
olyx replay trace_01HYW8A4Q9F7G9N2M3P4
```

Replay execution runs inside the customer environment when an edge node is
present. Olyx Cloud coordinates job state without storing provider secrets,
prompts, or model responses.

## Authentication

Use a project-scoped Olyx API key.

```sh
export OLYX_API_KEY="ak_..."
olyx config
```

You can also pass the key for one command:

```sh
olyx config --api-key "ak_..."
```

In production, use environment variables or a secrets manager — do not commit
API keys.

## Dry Run

Use `--dry-run` to verify request shape without sending a network request.

```sh
olyx trace \
  --dry-run \
  --metadata '{"environment":"ci","service":"checkout","operation":"smoke"}'
```

Dry-run output is redacted. Token-like strings and sensitive keys such as
`api_key`, `authorization`, and `secret` are masked before printing.

## API Contract

The CLI uses a JSON API over HTTPS.

| Command | Method | Endpoint |
| :--- | :--- | :--- |
| `olyx config` | `GET` | `/api/v1/config` |
| `olyx trace` | `POST` | `/api/v1/traces` |
| `olyx complete` | `PATCH` | `/api/v1/traces/:id/complete` |
| `olyx replay` | `POST` | `/api/v1/replay` |

Every request sends the project key as:

```http
Authorization: Bearer $OLYX_API_KEY
```

Errors use this shape:

```json
{
  "error": "message",
  "code": "machine_code",
  "request_id": "req_..."
}
```

## CI Smoke Check

```yaml
- name: Olyx CLI smoke check
  env:
    OLYX_API_KEY: ${{ secrets.OLYX_API_KEY }}
  run: |
    olyx inspect
    olyx trace --metadata '{"environment":"ci","service":"checkout","operation":"deploy.verify"}'
```

## Environment Variables

**CLI (all deployments)**

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_BASE_URL` | No | Olyx endpoint. Cloud: `https://app.olyxai.io` (default). Enterprise: your `olyx-edge` host. |
| `OLYX_API_KEY` | Yes | Project-scoped Olyx API key. |

**Edge node (enterprise, set on the `olyx-edge` process)**

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_EDGE_NODE_SECRET` | Yes | Node credential issued by Olyx. Format: `en_<id>.<64-hex-secret>`. |
| `OLYX_GATEWAY_URL` | Yes | Upstream model provider gateway URL. |
| `OLYX_API_BASE_URL` | Yes | Olyx API URL for edge config and heartbeat. |
| `OLYX_CLOUD_SYNC_URL` | No | If set, trace events are forwarded to Olyx Cloud as a secondary path. Omit for fully air-gapped operation. |
| `OLYX_LISTEN_ADDR` | No | Address the edge listens on. Defaults to `0.0.0.0:7878`. |

## Privacy and Security Boundary

Provider keys, prompts, and model responses stay in your environment. The CLI
only sends what your project policy allows: trace metadata, local references,
and trace lifecycle events.

When `olyx-edge` is deployed, policy enforcement and guardrails run inside your
infrastructure before any data leaves your network. `olyx-core` ensures the
edge node cannot operate with an unsigned, tampered, or expired policy pack.

## Release Channels

| Channel | Status |
| :--- | :--- |
| GitHub Releases | Available |
| Homebrew tap | Coming soon |
| Chocolatey | Coming soon |

Release binaries are built in GitHub Actions across five targets, signed with
Sigstore, and published with SHA-256 checksums. The pipeline uses only
official GitHub-maintained actions and plain `cargo` / `rustup` / `gh` shell
steps — no third-party action wrappers.

## Local Development

```sh
cargo build --bin olyx --release
./target/release/olyx help
```

Quality checks (run these before opening a patch):

```sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny --config deny.toml check advisories licenses bans sources
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full CI and release pipeline.

## Repository Policy

Issues are open. External pull requests are closed during the beta period.
