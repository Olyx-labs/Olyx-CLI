# Releasing the public CLI

Distribution is through this GitHub repository and signed GitHub Release binaries.
The crates are intentionally not published to crates.io.

## Prepare and rehearse

1. Keep the CLI version, changelog, supported-version policy, and workflow
   rehearsal tag aligned. The prepared release is `v0.2.0`.
2. Run formatting, Clippy, all workspace tests, and cargo-deny as listed in CONTRIBUTING.md.
3. Build from a clean standalone source export, with no sibling repositories.
4. Commit the reviewed public files; exclude credentials, `.env`, trace databases,
   customer payloads, compiled output, and private runtime code.
5. Push the release preparation branch and run the **Release CLI** workflow
   manually with `release_tag=v0.2.0-rehearsal` and `rehearsal=true`.
6. Review artifacts for all three targets, their checksums and Sigstore bundles,
   and the packaged examples, LICENSE, NOTICE, and documentation.
   Verify that the release smoke tests cover streamed routing and the plugin contract.

A manual workflow run builds and verifies artifacts but does not publish a release.
No private repository token is required. The workflow uses GitHub's token and OIDC
identity for signing. Maintain the existing Actions permissions and pinned actions.

## Publish

After the rehearsal passes, merge the reviewed changes and create/push the
matching version tag. A push of `v0.2.0` triggers GitHub Release publication.
Changing repository visibility is a separate owner action; review history before
making a previously private repository public.

Consumers should verify the adjacent checksum and Sigstore bundle before running
a release binary. See README for installation and local usage.

## Readiness boundaries

A successful local check does not replace the three-target signed GitHub rehearsal.
The source and artifacts must pass that rehearsal before creating the release tag.

## Package a local build

Release builders need Rust 1.88+ and a C compiler; end users do not. Package an already-built executable with:

```sh
RUSTFLAGS="--remap-path-prefix=$HOME=/build" cargo +1.88.0 build --locked --release --bin oy
python3 scripts/package-release.py --binary target/release/oy --version v0.2.0 --target aarch64-apple-darwin --output dist
python3 scripts/validate-archive.py dist/olyx-v0.2.0-aarch64-apple-darwin.tar.gz
```

Use the target matching the binary you built. CI uses this same packager for all three platforms and adds Sigstore signatures before publication. Local packages have checksums but are not signed release artifacts. The path-remapping
flag keeps your home directory out of embedded Rust source locations.

For optional provenance verification, download the archive and its `.bundle`, then use:

```sh
cosign verify-blob --bundle ARCHIVE.bundle \
  --certificate-identity 'https://github.com/Olyx-labs/Olyx-CLI/.github/workflows/release.yml@refs/tags/v0.2.0' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com ARCHIVE
```

Replace the archive and tag with the release you downloaded.

## npm distribution

The release workflow also produces four npm tarballs: `olyx-cli` and one
optional native package per supported platform. Each native job tests a fresh
npx installation against a temporary local registry, with install hooks disabled.
The combined `olyx-npm-TAG` artifact contains the packages ready for publication.
Rehearsal runs prepare these packages but never publish them to npm.

Before publishing, sign in to npm and confirm that your account can publish
the launcher and all three platform package names.
Use the packages from the reviewed run. Publish all three native packages first,
then the launcher so its exact-version dependencies are available:

```sh
npm whoami
for package in dist/npm/olyx-cli-*-0.2.0.tgz; do
  npm publish "$package" --access public
done
npm publish dist/npm/olyx-cli-0.2.0.tgz --access public
```

npm publishing is a separate maintainer action from GitHub Release publication.
Keep npm and Cargo versions aligned. For trusted publishing, configure each npm
package for this repository and workflow before replacing manual publication.
