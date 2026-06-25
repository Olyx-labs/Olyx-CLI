# Contributing

Thanks for taking a look at `olyx-cli`.

## Current Contribution Policy

This repository is public for transparency and installation, but external code
contributions are closed for now while the CLI surface and private runtime
boundary continue to settle.

We are not accepting unsolicited pull requests at this stage.

## What To Open

- Bug reports: use the bug-report issue template.
- Feature requests: use the feature-request issue template.
- Security reports: follow [SECURITY.md](SECURITY.md) and email them privately.

## Discussions and Support

GitHub Discussions should remain disabled for now.

Questions, beta access, and product feedback should go through the published
Olyx docs and access channels rather than ad hoc GitHub threads.

## If You Still Want To Prepare A Patch

If we ask you to collaborate on a change directly:

1. Work from a small, focused branch.
2. Keep command help, README examples, and tests in sync.
3. Do not add private runtime logic, signing material, or customer secrets to
   this repository.

## CI and Release Pipeline

### Quality gate

The quality job runs on every release tag and `workflow_dispatch`. It builds a
temporary workspace that pairs this repo with the private `Olyx-core` repo,
then runs:

```sh
cargo fmt --package olyx-cli -- --check
cargo clippy --locked -p olyx-cli --all-targets -- -D warnings
cargo test --locked -p olyx-cli -p olyx-core
cargo deny --config deny.toml check advisories licenses bans sources
```

To replicate locally (requires the private core checked out as a sibling at
`../Olyx-core`):

```sh
cargo fmt --package olyx-cli -- --check
cargo clippy -p olyx-cli --all-targets -- -D warnings
cargo test -p olyx-cli
cargo deny --config deny.toml check advisories licenses bans sources
```

### Build matrix

| Target | Runner | Cross-compiler |
| :--- | :--- | :--- |
| `x86_64-unknown-linux-musl` | ubuntu-22.04 | `cross` |
| `aarch64-unknown-linux-musl` | ubuntu-22.04 | `cross` |
| `x86_64-apple-darwin` | macos-15 | — |
| `aarch64-apple-darwin` | macos-15 | — |
| `x86_64-pc-windows-msvc` | windows-2022 | — |

Linux binaries are statically linked via musl. No runtime libc or OpenSSL
dependency on the host.

### Tool versions

All pinned tool versions live as top-level `env:` vars in
`.github/workflows/release.yml`:

```yaml
RUST_VERSION: "1.88"
CARGO_DENY_VERSION: "0.16.4"
COSIGN_VERSION: "2.4.3"
```

Update them there. The workflow installs each tool via `rustup`, `cargo
install`, or direct binary download — no third-party GitHub Actions wrappers.

### Actions dependency surface

Only official GitHub-maintained actions are used:

- `actions/checkout`
- `actions/cache` — **pin to a commit SHA before the next production release**
- `actions/upload-artifact`
- `actions/download-artifact`

Everything else (`rustup`, `cargo deny`, `cosign`, `gh release`) runs as plain
shell steps.

### Cutting a release

1. Bump `version` in `Cargo.toml`.
2. Push a `vMAJOR.MINOR.PATCH` tag.
3. The workflow validates the tag matches `Cargo.toml`, builds all five
   targets, signs each archive with Sigstore, and publishes the GitHub Release.

Use `workflow_dispatch` with `rehearsal: true` to do a dry run that builds and
signs artifacts without publishing.
