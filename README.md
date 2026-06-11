# Olyx CLI

`olyx` is the open-source control-plane CLI for [Olyx](https://olyxai.io) — policy enforcement, intelligent routing, and a tamper-proof audit trail for every AI request. One base URL change. No library to install.

```sh
export OLYX_API_KEY="ak_..."

olyx inspect          # verify your environment
olyx config           # fetch active project policy
olyx trace --metadata '{"service":"checkout","operation":"cart.create"}'
olyx complete trace_01J6AF...
olyx replay  trace_01J6AF...
```

| Component | Visibility | Role |
| :--- | :--- | :--- |
| **olyx-cli** (this repo) | Open source | Thin client — trace, complete, replay, config |
| **olyx-edge** | Private | Enterprise data plane — deployed inside your infrastructure |
| **olyx-core** | Private | Cryptographic runtime — license and policy-pack verification |

- License: [Apache-2.0](LICENSE)
- Security: [SECURITY.md](SECURITY.md)
- Changelog: [GitHub Releases](https://github.com/Olyx-labs/Olyx-CLI/releases)

---

## Contents

- [Install](#install)
- [How it fits together](#how-it-fits-together)
- [Commands](#commands)
- [Authentication](#authentication)
- [Response shapes](#response-shapes)
- [Error handling](#error-handling)
- [Environment variables](#environment-variables)
- [CI integration](#ci-integration)
- [Local development](#local-development)

---

## Install

Download the archive for your platform from the [latest GitHub Release](https://github.com/Olyx-labs/Olyx-CLI/releases). Verify the checksum before installing — each archive ships with a `.sha256` file and a Sigstore bundle.

### macOS (Apple Silicon)

```sh
version="v0.1.0"
asset="olyx-${version}-aarch64-apple-darwin.tar.gz"

curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}.sha256"
shasum -a 256 -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-aarch64-apple-darwin/olyx" /usr/local/bin/olyx
olyx --version
```

### macOS (Intel)

```sh
version="v0.1.0"
asset="olyx-${version}-x86_64-apple-darwin.tar.gz"

curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}.sha256"
shasum -a 256 -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-x86_64-apple-darwin/olyx" /usr/local/bin/olyx
olyx --version
```

### Linux x86_64

Static build — no glibc or OpenSSL required on the host.

```sh
version="v0.1.0"
asset="olyx-${version}-x86_64-unknown-linux-musl.tar.gz"

curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}.sha256"
sha256sum -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-x86_64-unknown-linux-musl/olyx" /usr/local/bin/olyx
olyx --version
```

### Linux arm64

```sh
version="v0.1.0"
asset="olyx-${version}-aarch64-unknown-linux-musl.tar.gz"

curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}.sha256"
sha256sum -c "${asset}.sha256"

tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-aarch64-unknown-linux-musl/olyx" /usr/local/bin/olyx
olyx --version
```

### Windows (x86_64)

Run in PowerShell:

```powershell
$version = "v0.1.0"
$asset   = "olyx-$version-x86_64-pc-windows-msvc.zip"

Invoke-WebRequest -Uri "https://github.com/Olyx-labs/Olyx-CLI/releases/download/$version/$asset" -OutFile $asset
Invoke-WebRequest -Uri "https://github.com/Olyx-labs/Olyx-CLI/releases/download/$version/$asset.sha256" -OutFile "$asset.sha256"

(Get-FileHash $asset -Algorithm SHA256).Hash.ToLower() + "  " + $asset |
  Compare-Object (Get-Content "$asset.sha256")

Expand-Archive $asset -DestinationPath .
Move-Item "olyx-$version-x86_64-pc-windows-msvc\olyx.exe" "$env:LOCALAPPDATA\Microsoft\WindowsApps\olyx.exe"
olyx --version
```

### Verify provenance

Every release archive includes a Sigstore bundle. Verify before running the binary in sensitive environments:

```sh
cosign verify-blob \
  --bundle "${asset}.bundle" \
  --certificate-identity-regexp 'https://github.com/Olyx-labs/Olyx-CLI/.github/workflows/release.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "${asset}"
```

### Coming soon

| Channel | Status |
| :--- | :--- |
| Homebrew tap | Coming soon |
| Chocolatey | Coming soon |

---

## How it fits together

The CLI is a stateless thin client. `OLYX_BASE_URL` is the only routing knob — point it at Olyx Cloud for SaaS or at an `olyx-edge` node for enterprise. The commands, flags, and response shapes are identical in both modes.

**Cloud**

```
OLYX_BASE_URL=https://app.olyxai.io  (default)

Your app  ──▶  model provider   (direct — provider keys stay in your env)
olyx-cli  ──▶  Olyx Cloud       (POST /api/v1/traces, POST /api/v1/replay, …)
```

**Enterprise (self-hosted edge)**

```
OLYX_BASE_URL=https://olyx-edge.internal

Your app  ──▶  olyx-edge (/v1/chat/completions)
                  │  policy enforcement, PII masking, provider routing
                  │  signed and verified by olyx-core
                  ▼
               model provider

olyx-cli  ──▶  olyx-edge (/api/v1/*)
                  │  config, traces, complete, replay — served locally
                  ▼
               (optional) Olyx Cloud sync via OLYX_CLOUD_SYNC_URL
```

To route application traffic through an edge node, update your provider client's base URL:

```python
# Python — OpenAI SDK
client = OpenAI(
    api_key=os.environ["OPENAI_API_KEY"],
    base_url=os.environ["OLYX_BASE_URL"] + "/v1",
)
```

```typescript
// TypeScript — OpenAI SDK
const client = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY,
  baseURL: process.env.OLYX_BASE_URL + "/v1",
});
```

```ruby
# Ruby — OpenAI client
client = OpenAI::Client.new(
  access_token: ENV["OPENAI_API_KEY"],
  uri_base:     ENV["OLYX_BASE_URL"] + "/v1",
)
```

Provider keys never leave your environment. Enterprise edge nodes are fully air-gapped by default — set `OLYX_CLOUD_SYNC_URL` only if you want trace events forwarded to Olyx Cloud as a secondary path.

---

## Commands

### `olyx inspect`

Checks your local CLI configuration. Makes no network requests.

```sh
olyx inspect
```

```json
{
  "base_url": "https://app.olyxai.io",
  "api_key_present": true,
  "dry_run": false,
  "secrets_boundary": "customer_environment",
  "core_runtime": {
    "license_path_present": false,
    "policy_pack_path_present": false,
    "key_id_present": false,
    "public_key_present": false
  }
}
```

`api_key_present: true` and `secrets_boundary: customer_environment` confirm the CLI is ready. The `core_runtime` fields are relevant only for enterprise edge deployments.

---

### `olyx config`

Fetches the active project policy from the control plane.

```sh
olyx config
```

```json
{
  "offline_testing": false,
  "enterprise_compute": false,
  "guardrails": { "pii_redaction": true, "content_moderation": false },
  "runtime_failover": { "enabled": true, "fallback_provider": "anthropic" },
  "distribution": {
    "distribution_channel": "cli",
    "telemetry_mode": "public",
    "local_ref_required": false
  },
  "runtime_activation": { "status": "active" }
}
```

Run this on startup to validate that the policy your application received is one it knows how to handle. See [Policy Schema](https://olyxai.io/docs/v1/policy-schema) for the full field reference.

---

### `olyx trace`

Records trace metadata before a model call.

```sh
olyx trace --metadata '{
  "environment": "production",
  "service":     "support-agent",
  "operation":   "ticket.reply",
  "model":       "gpt-4.1-mini"
}'
```

```json
{
  "id":         "trace_01J6AF...",
  "status":     "pending",
  "created_at": "2026-06-11T09:00:00Z",
  "metadata":   { "environment": "production", "service": "support-agent", "operation": "ticket.reply", "model": "gpt-4.1-mini" },
  "revenue":    null,
  "distribution": { "distribution_channel": "cli", "telemetry_mode": "public", "local_ref_required": false }
}
```

Pass the returned `id` to `olyx complete` when the request finishes.

When routing through `olyx-edge`, the edge creates traces automatically as requests pass through. Use `olyx trace` directly only when you need explicit control over what metadata reaches the control plane, or when your application does not route through an edge node.

#### Attach a local reference

When the full request payload lives in your own storage, attach an opaque pointer instead of sending the payload:

```sh
olyx trace \
  --local-ref "s3://acme-private-traces/prod/trace_01HYW8.json" \
  --metadata '{"environment":"production","service":"claims-agent","operation":"claim.review"}'
```

The reference should only resolve inside your environment. Do not use a public URL unless you intend that object to be reachable publicly.

---

### `olyx complete`

Marks a trace as complete. The server computes cost and latency summaries from the recorded steps.

```sh
olyx complete trace_01J6AF...
```

When routing through `olyx-edge`, the edge sends completion automatically. Call this directly when managing traces without an edge node.

---

### `olyx replay`

Queues a replay job for a completed trace.

```sh
olyx replay trace_01J6AF...
```

```json
{
  "job_id":          "9f4c2a8d1e7b4a6c",
  "source_trace_id": "trace_01J6AF...",
  "status":          "queued"
}
```

Poll `GET /api/v1/replay/:job_id` to check progress:

```json
{ "status": "running" }
```

| Status | Meaning |
| :--- | :--- |
| `queued` | Job accepted, not yet started |
| `running` | Replay is executing |
| `completed` | Replay finished — results visible in the dashboard |
| `failed` | Replay could not complete |
| `queued_for_agent` | Enterprise edge present — job runs inside your environment |

Results appear in the dashboard once the job reaches `completed`. Use replay to compare routing decisions or model behavior after a policy change without re-running the original request.

---

### `olyx login`

Opens the Olyx dashboard sign-in page in your browser.

```sh
olyx login
```

To print the URL without opening a browser:

```sh
olyx login --dry-run
```

---

## Authentication

All commands authenticate with a project-scoped API key.

```sh
export OLYX_API_KEY="ak_..."
olyx config
```

To pass the key for a single command without setting the environment variable:

```sh
olyx config --api-key "ak_..."
```

In production, read the key from your secrets manager — do not commit it or echo it in logs.

Every request sends:

```http
Authorization: Bearer <OLYX_API_KEY>
```

---

## Response shapes

### Trace created — `POST /api/v1/traces`

```json
{
  "id":           "trace_01J6AF...",
  "status":       "pending",
  "created_at":   "2026-06-11T09:00:00Z",
  "metadata":     {},
  "revenue":      null,
  "distribution": {
    "distribution_channel": "cli",
    "telemetry_mode":       "public",
    "local_ref_required":   false
  }
}
```

### Replay queued — `POST /api/v1/replay`

```json
{
  "job_id":          "9f4c2a8d1e7b4a6c",
  "source_trace_id": "trace_01J6AF...",
  "status":          "queued"
}
```

### Replay status — `GET /api/v1/replay/:job_id`

```json
{ "status": "running" }
```

---

## Error handling

All errors use this shape:

```json
{
  "error":      "human-readable message",
  "code":       "machine_readable_code",
  "request_id": "req_..."
}
```

| HTTP status | When it occurs |
| :--- | :--- |
| `401` | Missing or invalid API key |
| `403` | Key does not have access to this project |
| `404` | Trace or job ID not found |
| `422` | Invalid request body — `error` field describes the problem |
| `429` | Rate limit exceeded — back off and retry |
| `503` | Control plane temporarily unavailable |

Retry `503` responses with exponential backoff. Treat `401` and `403` as configuration errors — do not retry.

---

## Environment variables

### CLI (all deployments)

| Variable | Required | Default | Description |
| :--- | :--- | :--- | :--- |
| `OLYX_API_KEY` | Yes | — | Project-scoped API key |
| `OLYX_BASE_URL` | No | `https://app.olyxai.io` | Control plane endpoint. Set to your `olyx-edge` host for enterprise. |

### Edge node (enterprise — set on the `olyx-edge` process)

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_EDGE_NODE_SECRET` | Yes | Node credential issued by Olyx. Format: `en_<id>.<64-hex-secret>` |
| `OLYX_GATEWAY_URL` | Yes | Upstream model provider gateway URL |
| `OLYX_API_BASE_URL` | Yes | Olyx API URL for edge config and heartbeat |
| `OLYX_CLOUD_SYNC_URL` | No | If set, trace events are forwarded to Olyx Cloud as a secondary path. Omit for fully air-gapped operation. |
| `OLYX_LISTEN_ADDR` | No | Address the edge node listens on. Defaults to `0.0.0.0:7878` |

---

## Dry run

Use `--dry-run` to verify request shape without making a network call.

```sh
olyx trace \
  --dry-run \
  --metadata '{"environment":"ci","service":"checkout","operation":"smoke"}'
```

Dry-run output is redacted — token-like strings and keys named `api_key`, `authorization`, and `secret` are masked before printing.

---

## CI integration

Add a smoke check to your deployment pipeline to confirm the CLI is connected before your application starts taking traffic:

```yaml
- name: Olyx smoke check
  env:
    OLYX_API_KEY: ${{ secrets.OLYX_API_KEY }}
  run: |
    olyx inspect
    olyx trace \
      --metadata '{"environment":"ci","service":"checkout","operation":"deploy.verify"}'
```

For ephemeral CI environments where you want to verify request shape without hitting the API, add `--dry-run`.

---

## API contract

| Command | Method | Endpoint |
| :--- | :--- | :--- |
| `olyx config` | `GET` | `/api/v1/config` |
| `olyx trace` | `POST` | `/api/v1/traces` |
| `olyx complete TRACE_ID` | `PATCH` | `/api/v1/traces/:id/complete` |
| `olyx replay TRACE_ID` | `POST` | `/api/v1/replay` |
| replay poll | `GET` | `/api/v1/replay/:job_id` |

---

## Local development

```sh
cargo build --bin olyx --release
./target/release/olyx help
```

Run the full quality suite before opening a patch:

```sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check --config deny.toml advisories licenses bans sources
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the CI and release pipeline.

---

## Privacy and security boundary

The CLI only sends what your project policy permits: trace metadata, local references, and lifecycle events. Provider keys, raw prompts, and model responses stay in your environment in both cloud and enterprise modes.

For enterprise deployments, `olyx-edge` applies policy enforcement and guardrails inside your infrastructure before any data leaves your network. `olyx-core` ensures the edge node cannot operate with an unsigned, tampered, or expired policy pack — built to meet EU AI Act Article 12 record-keeping requirements out of the box.

---

## Repository policy

Issues are open. External pull requests are closed during the beta period.
