# Releasing the public CLI

Distribution is through this GitHub repository and signed GitHub Release binaries.
The crates are intentionally not published to crates.io.

## Prepare and rehearse

1. Keep the CLI version, changelog, supported-version policy, and workflow
   rehearsal tag aligned. The prepared release is `v0.2.0`.
2. Run formatting, Clippy, all workspace tests, and cargo-deny as listed in README.
3. Build from a clean standalone source export, with no sibling repositories.
4. Commit the reviewed public files; exclude credentials, `.env`, trace databases,
   customer payloads, compiled output, and private runtime code.
5. Push the release preparation branch and run the **Release CLI** workflow
   manually with `release_tag=v0.2.0-rehearsal` and `rehearsal=true`.
6. Review artifacts for all five targets, their checksums and Sigstore bundles,
   and the packaged examples, LICENSE, NOTICE, and documentation.

A manual workflow run builds and verifies artifacts but does not publish a release.
No private repository token is required. The workflow uses GitHub's token and OIDC
identity for signing. Maintain the existing Actions permissions and pinned actions.

## Publish

After the rehearsal passes, merge the reviewed changes and create/push the
matching version tag. A push of `v0.2.0` triggers GitHub Release publication.
Changing repository visibility is a separate owner action; review history before
making a previously private repository public. This preparation does not change
visibility, push commits/tags, or publish artifacts.

Consumers should verify the adjacent checksum and Sigstore bundle before running
a release binary. See README for installation and local usage.
