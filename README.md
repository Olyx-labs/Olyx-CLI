# Olyx CLI

`olyx` is the command-line tool for managing Olyx from the command line.

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

Each release archive also includes a Sigstore bundle.

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
export OLYX_BASE_URL="https://app.olyxai.io"

olyx inspect
olyx config
olyx trace --metadata '{"environment":"production","service":"checkout","operation":"cart.create"}'
```

`OLYX_BASE_URL` defaults to `https://app.olyxai.io`.

## How Olyx Fits Your Model Calls

Your application still owns the model call. Olyx sits in the request path as the
control plane.

Before:

```python
client = ProviderAI(api_key=os.environ["PROVIDER_API_KEY"])
```

After:

```python
client = ProviderAI(
    api_key=os.environ["PROVIDER_API_KEY"],
    base_url="https://app.olyxai.io/api/v1"
)
```

The provider key stays in your environment. Olyx applies your policy rules and
records the request metadata needed for debugging and replay.

## Commands

```sh
olyx help
olyx login
olyx inspect
olyx config
olyx trace --metadata JSON
olyx trace --local-ref REF
olyx replay TRACE_ID
olyx verify-runtime --license PATH --policy PATH --key-id KID --public-key-file PATH
```

### `olyx login`

Opens the Olyx dashboard sign-in page in your browser.

```sh
olyx login
```

For local or VPC dashboards, point the command at another control-plane URL:

```sh
olyx login --base-url https://app.olyxai.io
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

### `olyx trace`

Sends trace metadata to Olyx.

```sh
olyx trace --metadata '{
  "environment": "production",
  "service": "support-agent",
  "operation": "ticket.reply",
  "model_provider": "openai",
  "model": "gpt-4.1-mini",
  "policy_version": "2026.05.27",
  "request_id": "req_01HYW7W1R3H8"
}'
```

Use this when Olyx should see request context but not the prompt, model
response, or provider key.

### `olyx trace --local-ref`

Use a local reference when the full trace lives in your own system.

```sh
olyx trace \
  --local-ref "s3://acme-private-traces/prod/trace_01HYW8.json" \
  --metadata '{
    "environment": "production",
    "service": "claims-agent",
    "operation": "claim.review",
    "replay_mode": "local_reference"
  }'
```

The reference should only resolve inside your environment. Do not use a public
URL unless you intentionally want that object publicly reachable.

### `olyx replay`

Requests replay coordination for a trace.

```sh
olyx replay trace_01HYW8A4Q9F7G9N2M3P4
```

For VPC deployments, replay execution normally happens inside the customer
environment. Olyx coordinates job state without storing provider secrets,
prompts, model responses, or private model credentials.

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

## Private Runtime Verification

VPC deployments can activate a private runtime with a signed license and a
signed policy pack.

```sh
export OLYX_LICENSE_PATH="./license.olyx.json"
export OLYX_POLICY_PACK_PATH="./policy-pack.olyx.json"
export OLYX_POLICY_KEY_ID="olyx-prod-2026-01"
export OLYX_POLICY_PUBLIC_KEY_PATH="./olyx-prod-2026-01.pub"

olyx verify-runtime
```

Or pass the files directly:

```sh
olyx verify-runtime \
  --license ./license.olyx.json \
  --policy ./policy-pack.olyx.json \
  --key-id olyx-prod-2026-01 \
  --public-key-file ./olyx-prod-2026-01.pub
```

The runtime blocks and refuses to start if a license or policy pack is missing,
unsigned, tampered with, expired, signed by the wrong key, bound to the wrong
project, or not permitted for CLI mode.

Successful verification prints a status object:

```json
{
  "active": true,
  "key_id": "olyx-prod-2026-01",
  "customer_id": "cus_acme",
  "project_id": "proj_acme_prod",
  "policy_id": "pol_acme_prod",
  "policy_version": "2026.05.27",
  "runtime_modes": ["cli", "edge"],
  "policy_rules": 12,
  "license_expires_at": "2027-05-27T00:00:00Z",
  "policy_expires_at": "2026-06-03T00:00:00Z"
}
```

### Signing Key Rotation

Each signing key has a key ID (`kid`).

For rotation, Olyx issues a new public key and signs future license and policy
packs with the new `kid`. Update `OLYX_POLICY_KEY_ID` and
`OLYX_POLICY_PUBLIC_KEY_PATH`, then run `olyx verify-runtime` before deploying
the new artifacts.

## API Contract

The CLI uses a JSON API over HTTPS.

| Command | Method | Endpoint |
| :--- | :--- | :--- |
| `olyx config` | `GET` | `/api/v1/config` |
| `olyx trace` | `POST` | `/api/v1/traces` |
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
    OLYX_BASE_URL: https://app.olyxai.io
  run: |
    olyx inspect
    olyx trace --metadata '{"environment":"ci","service":"checkout","operation":"deploy.verify"}'
```

For VPC deployments, verify the private runtime before deploying:

```yaml
- name: Verify Olyx private runtime
  env:
    OLYX_LICENSE_PATH: ./license.olyx.json
    OLYX_POLICY_PACK_PATH: ./policy-pack.olyx.json
    OLYX_POLICY_KEY_ID: olyx-prod-2026-01
    OLYX_POLICY_PUBLIC_KEY_PATH: ./olyx-prod-2026-01.pub
  run: olyx verify-runtime
```

## Environment Variables

| Variable | Required | Description |
| :--- | :--- | :--- |
| `OLYX_BASE_URL` | No | Olyx API URL. Defaults to `https://app.olyxai.io`. |
| `OLYX_API_KEY` | For cloud calls | Project-scoped Olyx API key. |
| `OLYX_LICENSE_PATH` | For private runtime | Path to signed license envelope JSON. |
| `OLYX_POLICY_PACK_PATH` | For private runtime | Path to signed policy-pack envelope JSON. |
| `OLYX_POLICY_KEY_ID` | For private runtime | Expected Olyx signing key ID. |
| `OLYX_POLICY_PUBLIC_KEY_B64` | For private runtime | Ed25519 public key as base64url or standard base64. |
| `OLYX_POLICY_PUBLIC_KEY_PATH` | For private runtime | File containing the Ed25519 public key. |

## Privacy and Security Boundary

Provider keys, prompts, and model responses stay in your environment. Olyx only
receives what your project policy allows. For VPC deployments, the private
runtime handles enforcement locally — nothing is sent to Olyx Cloud unless
explicitly configured.

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
cargo build -p olyx-cli --bin olyx --release
./target/release/olyx help
```

Quality checks (run these before opening a patch):

```sh
cargo fmt --package olyx-cli -- --check
cargo clippy -p olyx-cli --all-targets -- -D warnings
cargo test -p olyx-cli
cargo deny --config deny.toml check advisories licenses bans sources
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full CI and release pipeline.

## Repository Policy

Issues are open. External pull requests are closed during the beta period.
