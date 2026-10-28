# Contributing

Issues for local routing, replay, debugging, provider compatibility, and
documentation are welcome. External code contributions remain closed during
the beta period; prepare patches only when invited. Report security issues privately
using [SECURITY.md](SECURITY.md).

Keep command help, examples, and tests in sync. Tests should use local mock
providers; never require live credentials or private Olyx repositories.
Do not commit customer payloads, provider keys, signing keys, or proprietary
enterprise modules. The public source is Apache-2.0.

Node.js 22+ is needed for npm launcher and packaging changes.
Run these checks from the repository root before submitting a change:

```sh
node --test npm/launcher.test.cjs
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 scripts/verify-public-boundary.py
python3 scripts/test-providers
cargo deny check --config deny.toml advisories licenses bans sources
```

Install [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) to run the last
check. CI pins the audit tool version.

CI tests all workspace crates. Release artifacts retain checksum and Sigstore verification
through the existing release workflow.

## Independent provider adapters

Adapters can live in their own repositories and be written in any language. They
use the [versioned subprocess contract](docs/plugins.md), not private runtime
APIs. A core source change or PR is not required to configure an external adapter.
Use a fixture mode and `scripts/test-plugin` to check the contract without provider
accounts. A contract pass does not establish live service compatibility; document
that evidence separately. Do not add native vendor SDKs to the core workspace.
