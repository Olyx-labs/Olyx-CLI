# Security Policy

## Reporting a Vulnerability

Please do not open a public GitHub issue for a suspected vulnerability.

Report security issues privately to `mosesnjoroge@olyxai.io` with:

- a short summary of the issue
- affected version or commit
- reproduction steps or proof of concept
- impact assessment if known

We will acknowledge receipt as soon as possible and work with you on triage,
validation, and coordinated disclosure.

## Supported Scope

This repository covers the public `olyx` CLI command surface and release
packaging.

Private runtime internals in `olyx-core`, outbound agent internals in
`olyx-edge`, cloud-side signing systems, and deployment-specific customer
infrastructure are handled privately, but vulnerabilities affecting those areas
should still be reported through the same contact.

## Disclosure Guidance

- Do not post exploit details in public issues, pull requests, or discussions.
- Do not include live credentials, customer payloads, or private runtime
  artifacts in reports.
- If you need to share sensitive reproduction material, say so in the initial
  email and we will coordinate a safer handoff.
