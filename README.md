# Olyx CLI

Command-line client for the Olyx CLI and private-deployment distribution
channel.

`olyx` is intentionally thin. It handles command UX, local file loading,
diagnostics, and Olyx Cloud transport. Private policy enforcement, license
verification, signed policy-pack loading, scrubbing, and block decisions live in
the compiled `olyx-core` runtime.

## Install

Public installation commands will be added when the first CLI build is released.
For local development:

```sh
cargo build -p olyx-cli --release
./target/release/olyx help
```

## Quickstart

```sh
export OLYX_API_KEY="ak_..."
export OLYX_BASE_URL="https://app.olyxai.io"

olyx doctor
olyx config
olyx trace --metadata '{"environment":"production","service":"checkout","operation":"cart.create"}'
```

`OLYX_BASE_URL` defaults to `https://app.olyxai.io`. `gateway.olyxai.io` is
reserved for a future endpoint split.

## Commands

```sh
olyx help
olyx doctor
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

Use `trace --metadata` when Olyx Cloud should receive structured telemetry but
not the underlying prompt, completion, provider key, or customer payload.

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

This is the recommended default for public and pro telemetry. Olyx Cloud sees
the operational metadata needed for policy, analytics, and replay coordination;
the sensitive application data remains in your environment.

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

For zero-outbound enterprise deployments, replay execution should happen in the
customer environment through the private runtime or outbound agent. Olyx Cloud
can coordinate the replay, but it does not need to store provider secrets,
prompts, completions, or private model credentials.

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
the same `project_id`.

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
    olyx doctor
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
metadata and an opaque reference to Olyx:

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
    "privacy_mode": "zero_outbound",
    "provider": "customer_managed"
  }'
```

For pro anonymized telemetry:

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

For public telemetry:

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

The CLI must not collect or upload provider secrets. Provider keys, private model
tokens, prompts, completions, and customer trace payloads stay in the customer
environment.

Olyx Cloud receives policy/config requests, trace metadata, opaque local
references, and replay coordination requests. Private enforcement remains in the
compiled `olyx-core` runtime and, for enterprise deployments, the outbound agent.
