# Contributing

Issues for local routing, replay, debugging, provider compatibility, and
documentation are welcome. External code contributions remain closed during
the beta period; prepare patches only when invited. Report security issues privately
using [SECURITY.md](SECURITY.md).

Keep command help, examples, and tests in sync. Tests should use local mock
providers; never require live credentials or private Olyx repositories.
Do not commit customer payloads, provider keys, signing keys, or proprietary
enterprise modules. The public source is Apache-2.0.

Run the verification commands in README before submitting a change. CI tests
all workspace crates. Release artifacts retain checksum and Sigstore verification
through the existing release workflow.
