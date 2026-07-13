# Olyx CLI

[![CI](https://github.com/Olyx-labs/Olyx-CLI/actions/workflows/ci.yml/badge.svg)](https://github.com/Olyx-labs/Olyx-CLI/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Olyx-labs/Olyx-CLI)](https://github.com/Olyx-labs/Olyx-CLI/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> [!TIP]
> Questions about Olyx? Visit [olyxai.io](https://olyxai.io) or open an [issue](https://github.com/Olyx-labs/Olyx-CLI/issues). Security concerns? See [SECURITY.md](SECURITY.md).

`olyx` is the open-source control-plane CLI for [Olyx](https://olyxai.io) — policy enforcement, intelligent routing, and a tamper-proof audit trail for every AI request. One base URL change. No library to install.

The CLI is the fastest path. Every command also maps directly to an HTTP endpoint if you prefer to integrate without installing the binary.

**CLI**
```sh
export OLYX_API_KEY="ak_..."

olyx inspect          # verify your environment
olyx config           # fetch active project policy
olyx trace --metadata '{"service":"checkout","operation":"cart.create"}'
olyx complete trace_01J6AF...
olyx replay  trace_01J6AF...
```

**HTTP API** — no install required
```sh
export OLYX_API_KEY="ak_..."

curl https://app.olyxai.io/api/v1/config \
  -H "Authorization: Bearer $OLYX_API_KEY"

curl https://app.olyxai.io/api/v1/traces \
  -H "Authorization: Bearer $OLYX_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"metadata":{"service":"checkout","operation":"cart.create"}}'

curl -X PATCH https://app.olyxai.io/api/v1/traces/trace_01J6AF.../complete \
  -H "Authorization: Bearer $OLYX_API_KEY"

curl https://app.olyxai.io/api/v1/replay \
  -H "Authorization: Bearer $OLYX_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"trace_id":"trace_01J6AF..."}'
```

| Component | Visibility | Role |
| :--- | :--- | :--- |
| **olyx-cli** (this repo) | Open source | Command-line client for tracing, replaying requests, and managing project configuration |
| **olyx-edge** | Private | Enterprise data plane — deployed inside your infrastructure |

- License: [Apache-2.0](LICENSE)
- Security: [SECURITY.md](SECURITY.md)
- Changelog: [GitHub Releases](https://github.com/Olyx-labs/Olyx-CLI/releases)

---

## Contents

- [Requirements](#requirements)
- [Install](#install)
- [How it fits together](#how-it-fits-together)
- [Commands](#commands)
- [Authentication](#authentication)
- [Response shapes](#response-shapes)
- [Error handling](#error-handling)
- [Exit codes](#exit-codes)
- [Environment variables](#environment-variables)
- [Dry run](#dry-run)
- [CI integration](#ci-integration)
- [API contract](#api-contract)
- [Local development](#local-development)
- [Privacy and security boundary](#privacy-and-security-boundary)
- [Repository policy](#repository-policy)
- [Support](#support)

---

## Requirements

- **Supported platforms:** macOS (Intel & Apple Silicon), Linux (x86_64 & arm64), Windows (x86_64)
- **API key:** project-scoped — run `olyx login` to open the dashboard and generate one
- **Network:** outbound HTTPS to `app.olyxai.io` (SaaS) or your `olyx-edge` endpoint (enterprise)
- **Rust (building from source only):** 1.88 or newer

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

The CLI connects to a single endpoint: `OLYX_BASE_URL`. Point it at Olyx Cloud for SaaS or at an `olyx-edge` node for enterprise. The commands, flags, and response shapes are identical in both modes.

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

`api_key_present: true` confirms the key is present. `secrets_boundary: customer_environment` confirms provider credentials are not being sent to Olyx. `core_runtime` reports, without reading their contents, whether the enterprise license and signed policy-pack material referenced by [`OLYX_LICENSE_PATH`, `OLYX_POLICY_PACK_PATH`, `OLYX_POLICY_KEY_ID`, and `OLYX_POLICY_PUBLIC_KEY_B64`/`OLYX_POLICY_PUBLIC_KEY_PATH`](#environment-variables) are configured in this environment — useful for confirming an enterprise host is wired up correctly before it starts taking traffic. On a standard Olyx Cloud setup these are all `false`.

---

### `olyx config`

Fetches the active project policy from the control plane.

```sh
olyx config

# HTTP equivalent
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

# HTTP equivalent
curl https://app.olyxai.io/api/v1/traces \
  -H "Authorization: Bearer $OLYX_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "environment": "production",
      "service":     "support-agent",
      "operation":   "ticket.reply",
      "model":       "gpt-4.1-mini"
    }
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

# HTTP equivalent
curl -X PATCH https://app.olyxai.io/api/v1/traces/trace_01J6AF.../complete \
  -H "Authorization: Bearer $OLYX_API_KEY"
```

When routing through `olyx-edge`, the edge sends completion automatically. Call this directly when managing traces without an edge node.

---

### `olyx replay`

Queues a replay job for a completed trace.

```sh
olyx replay trace_01J6AF...

# HTTP equivalent
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

### `olyx help`, `--version`

```sh
olyx help                # general usage
olyx help trace          # per-command usage, e.g. inspect|login|config|trace|complete|replay
olyx --version           # or -V
```

Both run offline and never require `OLYX_API_KEY`.

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

> [!WARNING]
> Never commit `OLYX_API_KEY` to version control or echo it in logs. Read it from a secrets manager (AWS Secrets Manager, GitHub Actions secrets, HashiCorp Vault, etc.) in production.

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

## Exit codes

| Code | Meaning |
| :--- | :--- |
| `0` | Success |
| `1` | Request failed — network error, non-2xx HTTP response, or an API error body |
| `2` | Usage error — bad flags, invalid arguments, or a missing `OLYX_API_KEY` |

Scripts and CI steps can branch on these without parsing stderr: treat `2` as a fix-your-invocation error and `1` as a runtime/retry candidate.

---

## Environment variables

### CLI (all deployments)

| Variable | Required | Default | Description |
| :--- | :--- | :--- | :--- |
| `OLYX_API_KEY` | Yes | — | Project-scoped API key |
| `OLYX_BASE_URL` | No | `https://app.olyxai.io` | Control plane endpoint. Set to your `olyx-edge` host for enterprise. |

### CLI (enterprise runtime probes)

These are read locally by `olyx-cli` itself — not sent over the network — and only affect the `core_runtime` block in [`olyx inspect`](#olyx-inspect) output. They let you confirm an enterprise host has its license and policy-pack material in place before wiring up `olyx-edge`.

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_LICENSE_PATH` | No | Path to the enterprise license file |
| `OLYX_POLICY_PACK_PATH` | No | Path to the signed policy pack |
| `OLYX_POLICY_KEY_ID` | No | Key ID used to verify the policy pack signature |
| `OLYX_POLICY_PUBLIC_KEY_B64` | No | Base64-encoded public key for policy-pack verification |
| `OLYX_POLICY_PUBLIC_KEY_PATH` | No | Path to the public key file, as an alternative to `OLYX_POLICY_PUBLIC_KEY_B64` |

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

All CLI output — dry-run bodies, live responses, and error messages alike — is redacted before printing. See [Output redaction](#output-redaction) for the full scope.

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

| Command | Method | Endpoint | Body |
| :--- | :--- | :--- | :--- |
| `olyx config` | `GET` | `/api/v1/config` | — |
| `olyx trace` | `POST` | `/api/v1/traces` | `{"metadata": {...}}` |
| `olyx complete TRACE_ID` | `PATCH` | `/api/v1/traces/:id/complete` | — |
| `olyx replay TRACE_ID` | `POST` | `/api/v1/replay` | `{"trace_id": "..."}` |
| replay poll | `GET` | `/api/v1/replay/:job_id` | — |

---

## Local development

```sh
git clone https://github.com/Olyx-labs/Olyx-CLI.git
cd Olyx-CLI

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

For enterprise deployments, `olyx-edge` applies policy enforcement and guardrails inside your infrastructure before any data leaves your network. The edge node verifies it is running a signed, current policy pack before accepting traffic — built to meet EU AI Act Article 12 record-keeping requirements out of the box.

### Output redaction

Every string the CLI prints — successful responses, dry-run bodies, and error messages — passes through a redaction filter first, so a copy-pasted terminal log or CI artifact doesn't leak a credential:

- **By key name:** JSON fields such as `api_key`, `access_token`, `refresh_token`, `token`, `secret`, `client_secret`, `authorization`, `password`, `private_key`, `credentials`, `bearer_token`, `x_api_key`, and provider-specific variants (`openai_api_key`, `anthropic_api_key`, `gemini_api_key`) are replaced with `[REDACTED]`, regardless of `camelCase` or `snake_case` spelling.
- **By value shape:** token-like strings are masked by prefix wherever they appear in text, including `Authorization: Bearer ...` headers — covering Olyx (`ak_`), OpenAI (`sk-`), GitHub (`ghp_`, `gho_`, `ghs_`, `glpat-`), Slack (`xoxb-`, `xoxp-`, `xoxs-`, `xoxa-`), npm (`npm_`), SendGrid (`SG.`), and AWS (`AKIA`, `ASIA`) tokens.

This applies uniformly across cloud and enterprise modes and isn't a `--dry-run`-only behavior.

### SSRF protection on `--base-url` / `OLYX_BASE_URL`

The CLI refuses to send requests, including the bearer token, to cloud-metadata and link-local addresses (`169.254.169.254`, `metadata.google.internal`, `100.100.100.200`, and the broader `169.254.0.0/16` range) even if `OLYX_BASE_URL` is misconfigured to point at one. Plain `http://` is only accepted for `localhost`/`127.0.0.1`/`::1`; every other endpoint must use `https://`.

---

## Repository policy

> [!WARNING]
> External pull requests are closed during the beta period — see [CONTRIBUTING.md](CONTRIBUTING.md) before opening one. Issues are open.

---

## Support

- Documentation and product site: [olyxai.io](https://olyxai.io)
- Policy field reference: [Policy Schema](https://olyxai.io/docs/v1/policy-schema)
- Issues and questions: [GitHub Issues](https://github.com/Olyx-labs/Olyx-CLI/issues)
- Security reports: [SECURITY.md](SECURITY.md)

Versioning follows [SemVer](https://semver.org) — breaking changes land only in major version bumps.
