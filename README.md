# Olyx CLI

[![Version](https://img.shields.io/github/v/release/Olyx-labs/Olyx-CLI.svg)](https://github.com/Olyx-labs/Olyx-CLI/releases)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Build Status](https://github.com/Olyx-labs/Olyx-CLI/actions/workflows/ci.yml/badge.svg)](https://github.com/Olyx-labs/Olyx-CLI/actions)

> [!TIP]
> Questions about Olyx? Check the [docs](https://olyxai.io/docs) or open an [issue](https://github.com/Olyx-labs/Olyx-CLI/issues). Security concerns? See [SECURITY.md](SECURITY.md).

**Olyx** is the open-source control-plane CLI for [Olyx](https://olyxai.io) — policy enforcement, intelligent routing, and a tamper-proof audit trail for every AI request. One base URL change. No library upgrades.

The CLI is the fastest path. Every command also maps directly to an HTTP endpoint if you prefer integration without installing the binary.

---

## Quick Start

```bash
export OLYX_API_KEY="ak_..."

# Verify your setup (no network call)
olyx inspect

# Fetch the active policy for this project
olyx config

# Start a trace with metadata about your AI request
trace_id=$(olyx trace --metadata '{
  "service": "support-agent",
  "operation": "ticket.reply",
  "model": "gpt-4-turbo"
}' --json | jq -r '.id')

# ... your application makes an AI request through Olyx ...

# Mark it complete (server computes cost, latency)
olyx complete "$trace_id"

# Replay this trace to test policy changes without re-running the original request
olyx replay "$trace_id"
```

**Prefer HTTP?** Every command maps to a REST endpoint — see [API contract](#api-contract) below.

---

## Contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
  - [How it fits together](#how-it-fits-together)
  - [Commands](#commands)
  - [Authentication](#authentication)
- [Advanced](#advanced)
  - [Dry-run mode](#dry-run-mode)
  - [CI integration](#ci-integration)
  - [Local references](#local-references)
- [Reference](#reference)
  - [Response shapes](#response-shapes)
  - [Error handling](#error-handling)
  - [Environment variables](#environment-variables)
  - [API contract](#api-contract)
- [Development](#development)
- [Support](#support)

---

## Requirements

- **Supported platforms:** macOS (Intel & Apple Silicon), Linux (x86_64 & arm64), Windows (x86_64)
- **API key:** Project-scoped API key from [Olyx Dashboard](https://app.olyxai.io/dashboard)
- **Network:** Outbound HTTPS to `app.olyxai.io` (SaaS) or your `olyx-edge` endpoint (enterprise)
- **Rust (development only):** For building from source, Rust 1.70+

---

## Installation

Download the archive for your platform from the [latest GitHub Release](https://github.com/Olyx-labs/Olyx-CLI/releases). **Always verify the checksum before installing** — each archive ships with a `.sha256` file.

### macOS (Apple Silicon)

```bash
version="v0.1.0"
asset="olyx-${version}-aarch64-apple-darwin.tar.gz"

# Download the binary and checksum
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/Olyx-labs/Olyx-CLI/releases/download/${version}/${asset}.sha256"

# Verify the checksum
shasum -a 256 -c "${asset}.sha256"

# Extract and install
tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-aarch64-apple-darwin/olyx" /usr/local/bin/olyx

# Verify
olyx --version
```

### macOS (Intel)

```bash
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

Static build — no glibc or OpenSSL required.

```bash
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

```bash
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

# Download
Invoke-WebRequest -Uri "https://github.com/Olyx-labs/Olyx-CLI/releases/download/$version/$asset" -OutFile $asset
Invoke-WebRequest -Uri "https://github.com/Olyx-labs/Olyx-CLI/releases/download/$version/$asset.sha256" -OutFile "$asset.sha256"

# Verify checksum
(Get-FileHash $asset -Algorithm SHA256).Hash.ToLower() + "  " + $asset |
  Compare-Object (Get-Content "$asset.sha256")

# Extract and install
Expand-Archive $asset -DestinationPath .
Move-Item "olyx-$version-x86_64-pc-windows-msvc\olyx.exe" "$env:LOCALAPPDATA\Microsoft\WindowsApps\olyx.exe"
olyx --version
```

### Verify Provenance

Every release archive includes a Sigstore bundle. Verify before running in sensitive environments:

```bash
cosign verify-blob \
  --bundle "${asset}.bundle" \
  --certificate-identity-regexp 'https://github.com/Olyx-labs/Olyx-CLI/.github/workflows/release.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "${asset}"
```

### Coming Soon

| Channel | Status |
| --- | --- |
| Homebrew tap | Coming soon |
| Chocolatey | Coming soon |

---

## Configuration

All configuration is via environment variables or command-line flags. Set your API key before running any command:

```bash
export OLYX_API_KEY="ak_..."
```

| Variable | Required | Default | Use Case |
| --- | --- | --- | --- |
| `OLYX_API_KEY` | **Yes** | — | Authenticates all requests. Get it from [Olyx Dashboard](https://app.olyxai.io/dashboard) |
| `OLYX_BASE_URL` | No | `https://app.olyxai.io` | SaaS (cloud) or self-hosted `olyx-edge` endpoint |

To pass the key for a single command without setting the env var:

```bash
olyx config --api-key "ak_..."
```

> [!WARNING]
> Never commit `OLYX_API_KEY` to version control. Use a secrets manager (AWS Secrets Manager, GitHub Secrets, HashiCorp Vault, etc.) in production.

---

## Usage

### How it fits together

The CLI connects to a single endpoint: `OLYX_BASE_URL`. Point it at Olyx Cloud for SaaS or at an `olyx-edge` node for enterprise. Commands, flags, and response shapes are identical in both.

**Olyx Cloud (SaaS)**

```
OLYX_BASE_URL=https://app.olyxai.io  (default)

Your app  ──→  model provider        (direct — provider keys stay in your env)
olyx-cli  ──→  Olyx Cloud            (POST /api/v1/traces, POST /api/v1/replay, …)
```

**Enterprise (self-hosted edge)**

```
OLYX_BASE_URL=https://olyx-edge.internal

Your app  ──→  olyx-edge (/v1/chat/completions)
                  │  policy enforcement, PII masking, provider routing
                  ▼
               model provider

olyx-cli  ──→  olyx-edge (/api/v1/*)
                  │  config, traces, complete, replay — served locally
                  ▼
               (optional) Olyx Cloud sync via OLYX_CLOUD_SYNC_URL
```

**To route your app through Olyx:**

Update your OpenAI client's `base_url` to point at Olyx. Provider credentials stay in your environment.

```python
# Python
from openai import OpenAI

client = OpenAI(
    api_key=os.environ["OPENAI_API_KEY"],
    base_url=os.environ["OLYX_BASE_URL"] + "/v1"
)
```

```typescript
// TypeScript
import OpenAI from "openai";

const client = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY,
  baseURL: process.env.OLYX_BASE_URL + "/v1"
});
```

```ruby
# Ruby
require "openai"

client = OpenAI::Client.new(
  access_token: ENV["OPENAI_API_KEY"],
  uri_base:     ENV["OLYX_BASE_URL"] + "/v1"
)
```

Provider keys **never leave your environment**. Enterprise edge nodes are fully air-gapped by default — set `OLYX_CLOUD_SYNC_URL` only if you want trace events forwarded to Olyx Cloud as a secondary path.

---

### Commands

#### `olyx inspect`

Checks your local CLI configuration. **Makes no network requests.**

```bash
olyx inspect
```

```json
{
  "base_url": "https://app.olyxai.io",
  "api_key_present": true,
  "dry_run": false,
  "secrets_boundary": "customer_environment"
}
```

**What this tells you:**
- `api_key_present: true` — your `OLYX_API_KEY` is set and readable
- `secrets_boundary: customer_environment` — provider credentials are **not** being sent to Olyx

---

#### `olyx config`

Fetches the active project policy from the control plane.

```bash
olyx config

# or via HTTP
curl https://app.olyxai.io/api/v1/config \
  -H "Authorization: Bearer $OLYX_API_KEY"
```

```json
{
  "enterprise_compute": {
    "enabled": false,
    "mode": "standard"
  },
  "guardrails": {
    "max_input_length": 10000,
    "pii_threshold": 0.7,
    "injection_block": true,
    "secret_action": "alert"
  },
  "runtime_failover": {
    "preferred_runtime": "cloud",
    "data_boundary": "olyx_cloud",
    "fail_closed": false
  },
  "distribution": {
    "distribution_channel": "cli",
    "telemetry_mode": "public",
    "local_ref_required": false
  }
}
```

**Run this on startup** to validate that the policy your application received is one it knows how to handle. See [Policy Schema](https://olyxai.io/docs/v1/policy-schema) for the full field reference.

---

#### `olyx trace`

Records trace metadata before an AI request. Returns a `trace_id` you'll pass to `olyx complete` when done.

```bash
trace_id=$(olyx trace --metadata '{
  "environment": "production",
  "service":     "support-agent",
  "operation":   "ticket.reply",
  "model":       "gpt-4-turbo"
}' --json | jq -r '.id')

# or via HTTP
curl https://app.olyxai.io/api/v1/traces \
  -H "Authorization: Bearer $OLYX_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "environment": "production",
      "service":     "support-agent",
      "operation":   "ticket.reply",
      "model":       "gpt-4-turbo"
    }
  }'
```

```json
{
  "id":         "trace_01J6AF...",
  "status":     "pending",
  "created_at": "2026-06-11T09:00:00Z",
  "metadata":   { "environment": "production", "service": "support-agent", "operation": "ticket.reply", "model": "gpt-4-turbo" },
  "revenue":    null,
  "distribution": { "distribution_channel": "cli", "telemetry_mode": "public", "local_ref_required": false }
}
```

**When routing through `olyx-edge`:** The edge creates traces automatically. Use `olyx trace` directly only when you need explicit control over metadata.

---

#### `olyx complete`

Marks a trace as complete. The server computes cost and latency summaries from the recorded steps.

```bash
olyx complete trace_01J6AF...

# or via HTTP
curl -X PATCH https://app.olyxai.io/api/v1/traces/trace_01J6AF.../complete \
  -H "Authorization: Bearer $OLYX_API_KEY"
```

**When routing through `olyx-edge`:** Completion happens automatically. Call this directly only when managing traces without an edge node.

---

#### `olyx replay`

Queues a replay job for a completed trace. Use replay to test policy changes or debug routing decisions without re-running the original request.

```bash
olyx replay trace_01J6AF...

# or via HTTP
curl https://app.olyxai.io/api/v1/replay \
  -H "Authorization: Bearer $OLYX_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"trace_id":"trace_01J6AF..."}'

# Poll job status
curl https://app.olyxai.io/api/v1/replay/9f4c2a8d1e7b4a6c \
  -H "Authorization: Bearer $OLYX_API_KEY"
```

```json
{
  "job_id":          "9f4c2a8d1e7b4a6c",
  "source_trace_id": "trace_01J6AF...",
  "status":          "queued"
}
```

**Job statuses:**

| Status | Meaning |
| --- | --- |
| `queued` | Job accepted, not yet started |
| `running` | Replay is executing |
| `completed` | Replay finished — results visible in dashboard |
| `failed` | Replay could not complete |
| `queued_for_agent` | Enterprise edge present — job runs inside your environment |

Results appear in the dashboard once the job reaches `completed`.

---

#### `olyx login`

Opens the Olyx dashboard in your browser.

```bash
olyx login

# To print the URL without opening a browser
olyx login --dry-run
```

---

### Authentication

All commands authenticate with a project-scoped API key. Get your key from the [Olyx Dashboard](https://app.olyxai.io/dashboard).

Set it as an environment variable:

```bash
export OLYX_API_KEY="ak_..."
olyx config
```

Or pass it per-command:

```bash
olyx config --api-key "ak_..."
```

**In production,** read the key from your secrets manager. Every request sends:

```http
Authorization: Bearer <OLYX_API_KEY>
```

---

## Advanced

### Dry-run mode

Use `--dry-run` to verify request shape without making a network call. Useful for validating config in CI or testing environments.

```bash
olyx trace \
  --dry-run \
  --metadata '{"environment":"ci","service":"checkout","operation":"smoke"}'
```

**Output is redacted:** token-like strings and keys named `api_key`, `authorization`, and `secret` are masked before printing.

---

### CI integration

Add a smoke check to your deployment pipeline to confirm the CLI is connected before your application takes traffic:

```yaml
- name: Olyx smoke check
  env:
    OLYX_API_KEY: ${{ secrets.OLYX_API_KEY }}
  run: |
    # Verify local setup (no network call)
    olyx inspect
    
    # Verify connection to control plane and policy fetch
    olyx trace \
      --metadata '{"environment":"ci","service":"checkout","operation":"deploy.verify"}'
```

For ephemeral CI environments, add `--dry-run` to skip the network call:

```bash
olyx inspect --dry-run
olyx trace --dry-run --metadata '{"environment":"ci",...}'
```

---

### Local references

When the full request payload lives in your own storage, attach an opaque pointer instead of sending the payload:

```bash
olyx trace \
  --local-ref "s3://acme-private-traces/prod/trace_01HYW8.json" \
  --metadata '{"environment":"production","service":"claims-agent","operation":"claim.review"}'
```

**Important:** The reference should only resolve inside your environment. Do not use a public URL unless you intend that object to be reachable publicly.

---

## Reference

### Response shapes

#### Trace created — `POST /api/v1/traces`

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

#### Replay queued — `POST /api/v1/replay`

```json
{
  "job_id":          "9f4c2a8d1e7b4a6c",
  "source_trace_id": "trace_01J6AF...",
  "status":          "queued"
}
```

#### Replay status — `GET /api/v1/replay/:job_id`

```json
{ "status": "running" }
```

#### Error — any endpoint

```json
{
  "error":      "human-readable message",
  "code":       "machine_readable_code",
  "request_id": "req_..."
}
```

---

### Error handling

All errors use the shape shown above. Interpret HTTP status codes as follows:

| HTTP status | When it occurs | Action |
| --- | --- | --- |
| `401` | Missing or invalid API key | Check `OLYX_API_KEY` is set and valid |
| `403` | Key does not have access to this project | Verify the key is for the correct Olyx project |
| `404` | Trace or job ID not found | Ensure the trace/job ID is correct |
| `422` | Invalid request body | Check the `error` field in the response for details |
| `429` | Rate limit exceeded | Back off and retry with exponential backoff |
| `503` | Control plane temporarily unavailable | Retry with exponential backoff |

**Retry strategy:**
- `503` responses: retry with exponential backoff
- `401` and `403`: treat as configuration errors — do not retry

---

### Environment variables

**For the CLI (all deployments):**

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `OLYX_API_KEY` | **Yes** | — | Project-scoped API key from [Olyx Dashboard](https://app.olyxai.io/dashboard) |
| `OLYX_BASE_URL` | No | `https://app.olyxai.io` | Control plane endpoint. Set to your `olyx-edge` host for enterprise deployments. |

**For edge node (enterprise — set on the `olyx-edge` process):**

| Variable | Required | Description |
| --- | --- | --- |
| `OLYX_EDGE_NODE_SECRET` | **Yes** | Node credential issued by Olyx. Format: `en_<id>.<64-hex-secret>` |
| `OLYX_GATEWAY_URL` | **Yes** | Upstream model provider gateway URL |
| `OLYX_API_BASE_URL` | **Yes** | Olyx API URL for edge config and heartbeat |
| `OLYX_CLOUD_SYNC_URL` | No | If set, trace events are forwarded to Olyx Cloud as a secondary path. Omit for fully air-gapped operation. |
| `OLYX_LISTEN_ADDR` | No | Address the edge node listens on. Defaults to `0.0.0.0:7878` |

---

### API contract

Every CLI command maps to an HTTP endpoint. Use this reference if you prefer curl or another HTTP client.

| CLI Command | HTTP Method | Endpoint | Body |
| --- | --- | --- | --- |
| `olyx config` | `GET` | `/api/v1/config` | — |
| `olyx trace` | `POST` | `/api/v1/traces` | `{"metadata": {...}}` |
| `olyx complete TRACE_ID` | `PATCH` | `/api/v1/traces/:id/complete` | — |
| `olyx replay TRACE_ID` | `POST` | `/api/v1/replay` | `{"trace_id": "..."}` |
| (poll job) | `GET` | `/api/v1/replay/:job_id` | — |

---

## Development

### Set up your dev environment

```bash
# Clone and enter the repo
git clone https://github.com/Olyx-labs/Olyx-CLI.git
cd Olyx-CLI

# Build the CLI
cargo build --bin olyx --release

# Verify
./target/release/olyx --version
```

### Pre-commit checklist

Before submitting a pull request, run:

```bash
# Format code
cargo fmt -- --check

# Lint (abort on warnings)
cargo clippy --all-targets -- -D warnings

# Run tests
cargo test

# Audit dependencies (license & security)
cargo deny check --config deny.toml advisories licenses bans sources
```

Alternatively, run all in one go:

```bash
just test
```

(We use [just](https://github.com/casey/just) for convenience — see `justfile`.)

### Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines, the CI pipeline, and release workflow.

> [!WARNING]
> External pull requests from first-time contributors are currently on hold during the beta period. Please [open an issue](https://github.com/Olyx-labs/Olyx-CLI/issues) to discuss proposed changes first.

---

## Support

**Need help?**

- **Documentation:** [Olyx Docs](https://olyxai.io/docs) — guides, concepts, and architecture
- **API Reference:** [Policy Schema](https://olyxai.io/docs/v1/policy-schema) — full policy field reference
- **Issues & questions:** [GitHub Issues](https://github.com/Olyx-labs/Olyx-CLI/issues)
- **Security:** [SECURITY.md](SECURITY.md)

**Versioning:** We follow [semantic versioning](https://semver.org). Breaking changes only occur in major version bumps. New features and bug fixes are released on the latest major version.

**License:** [Apache 2.0](LICENSE)
**Changelog:** [GitHub Releases](https://github.com/Olyx-labs/Olyx-CLI/releases)

---

## Privacy and security boundary

The CLI only sends what your project policy permits: trace metadata, local references, and lifecycle events. **Provider keys, raw prompts, and model responses stay in your environment** in both cloud and enterprise deployments.

For enterprise deployments, `olyx-edge` applies policy enforcement and guardrails inside your infrastructure before any data leaves your network. The edge node verifies it is running a signed, current build before accepting requests.

---

## Component overview

| Component | Visibility | Role |
| --- | --- | --- |
| **olyx-cli** (this repo) | Open source | Command-line client for tracing, replaying requests, and managing project configuration |
| **olyx-edge** | Private | Enterprise data plane — deployed inside your infrastructure for policy enforcement and routing |
