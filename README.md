# Olyx CLI

Command-line client for the Olyx CLI and private-deployment distribution
channel.

- License: [Apache-2.0](LICENSE)
- Security: [SECURITY.md](SECURITY.md)
- Contribution policy: [CONTRIBUTING.md](CONTRIBUTING.md)

`olyx` is intentionally thin. It handles command UX, local file loading,
diagnostics, and Olyx Cloud transport. Private policy enforcement, license
verification, signed policy-pack loading, scrubbing, and block decisions live in
the compiled `olyx-core` runtime.

## Install

Download the archive for your platform from the latest GitHub Release, verify
the checksum, then install the `olyx` binary.

macOS Apple Silicon:

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

macOS Intel:

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

Linux x86_64:

```sh
version="v0.1.0"
asset="olyx-${version}-x86_64-unknown-linux-gnu.tar.gz"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}"
curl -fsSLO "https://github.com/olyxlabs/olyx-cli/releases/download/${version}/${asset}.sha256"
sha256sum -c "${asset}.sha256" || shasum -a 256 -c "${asset}.sha256"
tar -xzf "${asset}"
sudo install -m 0755 "olyx-${version}-x86_64-unknown-linux-gnu/olyx" /usr/local/bin/olyx
olyx --version
```

Each release also includes Sigstore signature and certificate files, plus a
release-wide `SHA256SUMS.txt` manifest:

```sh
cosign verify-blob \
  --certificate "${asset}.pem" \
  --signature "${asset}.sig" \
  --certificate-identity-regexp 'https://github.com/olyxlabs/olyx-cli/.github/workflows/release.yml@refs/tags/v[0-9]+\.[0-9]+\.[0-9]+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "${asset}"
```

For local development:

```sh
cargo build -p olyx-cli --bin olyx --release
./target/release/olyx help
```

Release binaries are published through GitHub Releases. The release workflow
builds the public CLI against the private compiled core in CI, verifies the
tag matches the crate version, smoke-tests the built binary, verifies Sigstore
provenance before publishing, and uploads target-triple archives with
per-archive SHA-256 checksums, a release-wide `SHA256SUMS.txt`, and Sigstore
signatures.

## Quickstart

```sh
export OLYX_API_KEY="ak_..."
export OLYX_BASE_URL="https://app.olyxai.io"

olyx inspect
olyx config
olyx trace --metadata '{"environment":"production","service":"checkout","operation":"cart.create"}'
```

`OLYX_BASE_URL` defaults to `https://app.olyxai.io`.

## Product Boundary

Olyx has one public command surface and three operating modes:

| Tier | Runtime path | Telemetry | Replay | Payload storage |
| :--- | :--- | :--- | :--- | :--- |
| Free | CLI + Olyx Cloud | Public telemetry | Cloud replay | Stored by Olyx Cloud according to project policy |
| Pro | CLI + Olyx Cloud | Anonymized telemetry | Cloud replay | Anonymized or policy-bounded cloud storage |
| Enterprise | CLI + private runtime + optional outbound/on-prem agent | Customer-managed by default | Customer-managed, with optional cloud coordination | Customer environment by default; cloud retention only when explicitly configured |

Across all tiers, provider secrets remain customer-side. Olyx Cloud does not
need OpenAI, Anthropic, Gemini, Bedrock, Azure, or private-model credentials in
order to issue config, accept traces, or coordinate replay.

Enterprise private payloads are not stored by Olyx Cloud unless the deployment
is explicitly configured to allow that retention boundary.

## Public API Contract

The CLI speaks a narrow JSON protocol over HTTPS:

| Command | Endpoint |
| :--- | :--- |
| `olyx config` | `GET /api/v1/sdk/config` |
| `olyx trace` | `POST /api/v1/traces` |
| `olyx replay` | `POST /api/v1/replay` |

Every cloud request sends the project key as `Authorization: Bearer $OLYX_API_KEY` and expects JSON. Public API errors use this shape:

```json
{ "error": "message", "code": "machine_code", "request_id": "req_..." }
```

CLI output is sanitized before printing. Token-like values, bearer credentials,
and sensitive JSON keys such as `api_key`, `authorization`, and `secret` are
redacted in dry-run output and surfaced error messages.

## Commands

```sh
olyx help
olyx inspect
olyx config
olyx trace --metadata JSON
olyx trace --local-ref REF
olyx replay TRACE_ID
olyx verify-runtime --license PATH --policy PATH --key-id KID (--public-key-b64 KEY | --public-key-file PATH)
```

## Authentication

Use a project-scoped Olyx API key. The CLI sends it as a bearer token to Olyx
Cloud.

```sh
export OLYX_API_KEY="ak_..."
olyx config
```

You can also pass the key for a single command:

```sh
olyx config --api-key "ak_..."
```

For production systems, prefer environment variables or a secrets manager. Avoid
committing keys to shell scripts, CI logs, or dotfiles.

## Metadata-Only Trace

Use `trace --metadata` when your project mode allows cloud telemetry and you
want Olyx Cloud to receive structured telemetry but not the underlying prompt,
completion, provider key, or customer payload.

```sh
olyx trace --metadata '{
  "environment": "production",
  "service": "support-agent",
  "operation": "ticket.reply",
  "model_provider": "openai",
  "model": "gpt-4.1-mini",
  "policy_version": "2026.05.27",
  "request_id": "req_01HYW7W1R3H8",
  "tenant_tier": "enterprise"
}'
```

This is the recommended default for Pro projects and for Enterprise projects
that expose cloud-safe operational metadata. Olyx Cloud sees the policy,
analytics, and replay-coordination fields it needs; sensitive application data
remains in your environment.

## Opaque Local References

Use `--local-ref` when the full trace or replay payload is stored locally, in
your VPC, or in your own object store.

```sh
olyx trace \
  --local-ref "s3://acme-private-traces/prod/2026/05/27/trace_01HYW8.json" \
  --metadata '{
    "environment": "production",
    "service": "claims-agent",
    "operation": "claim.review",
    "replay_mode": "local_reference"
  }'
```

The reference should be meaningful only inside your environment. Do not use a
public URL unless you intentionally want that data to be accessible outside your
network.

## Replay Coordination

Replay requests coordinate against an existing trace id or local reference.

```sh
olyx replay trace_01HYW8A4Q9F7G9N2M3P4
```

For Enterprise private deployments, replay execution should normally happen in
the customer environment through the private runtime or outbound agent. Olyx
Cloud can coordinate replay state and job metadata, but it does not need to
store provider secrets, prompts, completions, or private model credentials
unless that deployment is explicitly configured to retain payloads in cloud.

## Private Runtime Verification

Private deployments activate the compiled core with a signed license and signed
policy pack.

```sh
export OLYX_LICENSE_PATH="./license.olyx.json"
export OLYX_POLICY_PACK_PATH="./policy-pack.olyx.json"
export OLYX_POLICY_KEY_ID="olyx-prod-2026-01"
export OLYX_POLICY_PUBLIC_KEY_PATH="./olyx-prod-2026-01.pub"

olyx verify-runtime
```

Or pass all inputs explicitly:

```sh
olyx verify-runtime \
  --license ./license.olyx.json \
  --policy ./policy-pack.olyx.json \
  --key-id olyx-prod-2026-01 \
  --public-key-file ./olyx-prod-2026-01.pub
```

The signed envelope shape is:

```json
{
  "protected": "base64url(header-json)",
  "payload": "base64url(payload-json)",
  "signature": "base64url(ed25519(protected + \".\" + payload))"
}
```

The compiled core refuses to activate unless the envelope uses `EdDSA`, the
`kid` matches the configured Olyx signing key, the signature verifies, the
license and policy pack are inside their validity windows, and both are bound to
the same `project_id`. The CLI also requires the signed license to include the
`cli` runtime mode; an `edge`-only or `failover`-only license will not activate
through `olyx verify-runtime`. Signed envelopes use a strict schema: unknown
top-level, protected-header, license, policy-pack, or policy-rule fields are
rejected instead of ignored.

Unsigned, malformed, oversized, expired, not-yet-valid, tampered, wrong-key,
wrong-project, or wrong-runtime artifacts fail closed and return a non-zero exit
code.

### Signing Key Rotation

Olyx signing keys are identified by `kid`. The customer runtime trusts one key
at a time by setting both the expected key id and the matching public key:

```sh
export OLYX_POLICY_KEY_ID="olyx-prod-2026-01"
export OLYX_POLICY_PUBLIC_KEY_PATH="./olyx-prod-2026-01.pub"
olyx verify-runtime
```

For rotation, Olyx issues a new public key and switches future license/policy
packs to the new `kid`. Customers update `OLYX_POLICY_KEY_ID` and
`OLYX_POLICY_PUBLIC_KEY_PATH`, then run `olyx verify-runtime` before deploying
the new artifacts. During a planned rotation window, keep both public keys in
your secrets manager, but configure each runtime with exactly the `kid` expected
for the artifacts it is activating.

Successful verification prints runtime status:

```json
{
  "active": true,
  "key_id": "olyx-prod-2026-01",
  "customer_id": "cus_acme",
  "project_id": "proj_acme_prod",
  "plan": "enterprise",
  "policy_id": "pol_acme_prod",
  "policy_version": "2026.05.27",
  "features": ["private_models", "policy_packs"],
  "runtime_modes": ["cli", "edge"],
  "policy_rules": 12,
  "license_expires_at": "2027-05-27T00:00:00Z",
  "policy_expires_at": "2026-06-03T00:00:00Z"
}
```

## CI Smoke Check

Use `--dry-run` to verify command shape without sending a request.

```sh
olyx trace \
  --dry-run \
  --metadata '{"environment":"ci","service":"checkout","operation":"smoke"}'
```

Example GitHub Actions step:

```yaml
- name: Olyx CLI smoke check
  env:
    OLYX_API_KEY: ${{ secrets.OLYX_API_KEY }}
    OLYX_BASE_URL: https://app.olyxai.io
  run: |
    olyx inspect
    olyx trace --metadata '{"environment":"ci","service":"checkout","operation":"deploy.verify"}'
```

For private deployments, add runtime verification before deploy:

```yaml
- name: Verify Olyx private runtime
  env:
    OLYX_LICENSE_PATH: ./license.olyx.json
    OLYX_POLICY_PACK_PATH: ./policy-pack.olyx.json
    OLYX_POLICY_KEY_ID: olyx-prod-2026-01
    OLYX_POLICY_PUBLIC_KEY_PATH: ./olyx-prod-2026-01.pub
  run: olyx verify-runtime
```

## Service Integration Examples

Wrap your provider call with your own local trace capture, then send only
metadata and an opaque reference to Olyx for Enterprise customer-managed
telemetry:

```sh
TRACE_REF="file:///var/lib/my-app/olyx/traces/trace_$(date +%s).json"

# Your application writes the full private trace to TRACE_REF.
# Olyx receives metadata plus the local reference only.
olyx trace \
  --local-ref "$TRACE_REF" \
  --metadata '{
    "environment": "production",
    "service": "research-assistant",
    "operation": "answer.generate",
    "privacy_mode": "customer_managed",
    "provider": "customer_managed"
  }'
```

For Pro anonymized telemetry:

```sh
olyx trace --metadata '{
  "environment": "production",
  "service": "research-assistant",
  "operation": "answer.generate",
  "privacy_mode": "anonymized",
  "prompt_class": "knowledge_search",
  "policy_outcome": "scrubbed"
}'
```

For Free public telemetry:

```sh
olyx trace --metadata '{
  "environment": "production",
  "service": "docs-chat",
  "operation": "question.answer",
  "privacy_mode": "public_telemetry"
}'
```

## Environment Variables

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_BASE_URL` | No | Olyx API URL. Defaults to `https://app.olyxai.io`. |
| `OLYX_API_KEY` | For Cloud calls | Project-scoped Olyx API key. |
| `OLYX_LICENSE_PATH` | For private runtime | Path to signed license envelope JSON. |
| `OLYX_POLICY_PACK_PATH` | For private runtime | Path to signed policy-pack envelope JSON. |
| `OLYX_POLICY_KEY_ID` | For private runtime | Expected Olyx signing key id. |
| `OLYX_POLICY_PUBLIC_KEY_B64` | For private runtime | Ed25519 public key as base64url or standard base64. |
| `OLYX_POLICY_PUBLIC_KEY_PATH` | For private runtime | File containing the Ed25519 public key. |

## Distribution Boundary

The CLI must not collect or upload provider secrets. Provider keys, private
model tokens, prompts, completions, and customer trace payloads stay in the
customer environment unless the project's cloud telemetry mode explicitly
permits bounded payload retention.

For Free and Pro projects, Olyx Cloud receives the policy/config calls, trace
data allowed by the project's telemetry mode, and replay coordination traffic.

For Enterprise private deployments, Olyx Cloud still handles project
configuration and optional replay coordination, but the private runtime owns the
enforcement path and payload boundary. The outbound/on-prem agent is optional
in that model and is used when the customer wants a local control-plane and
dashboard surface inside the private environment.

The public repository intentionally contains only the CLI command surface,
transport code, and release packaging. `olyx-core` source, policy evaluation
internals, scrubbing logic, block injection logic, signing keys, and license
issuance logic remain private. Public release binaries are built in CI from the
public CLI plus the private compiled core and are distributed only as signed,
checksummed archives.

## Repository Policy

This repository should use issue templates only, with blank issues disabled.

GitHub Discussions should remain disabled for now, and external pull requests
are closed while the public CLI surface continues to settle.

No release badge is included until the public release workflow has been stable
in production for a sustained period.
