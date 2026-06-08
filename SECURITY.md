# Security Policy

## Supported Versions

| Version | Supported |
| :--- | :--- |
| 0.1.x (current) | Yes |

Only the latest release receives security fixes. Update to the current release before reporting.

## Reporting a Vulnerability

**Do not open a public GitHub issue for a suspected vulnerability.**

Report security issues privately to `mosesnjoroge@olyxai.io` with:

- A short summary of the issue
- Affected version (`olyx --version`) and operating system
- Reproduction steps or proof of concept
- Impact assessment if known

You will receive an acknowledgement within **2 business days** and a resolution timeline within **7 business days**. Critical vulnerabilities are patched on an accelerated timeline.

## Disclosure Policy

We follow coordinated disclosure. We ask that you:

1. Allow us reasonable time to investigate and patch before public disclosure
2. Avoid accessing or modifying data that does not belong to you
3. Limit proof-of-concept reproduction to your own environment

We will credit researchers who report valid vulnerabilities unless they prefer to remain anonymous.

## Scope

This repository covers the public `olyx` CLI command surface and release packaging.

**In scope:**
- Credential leakage through CLI output, error messages, logs, or process arguments
- Bypass of signed-envelope verification in `olyx verify-runtime`
- Supply chain issues in published release artifacts (checksum, Sigstore bundle, GitHub Actions workflow)
- Privilege escalation or arbitrary code execution triggered by CLI input

**Out of scope:**
- Vulnerabilities in dependencies already publicly disclosed and tracked by cargo-audit
- Social engineering or phishing attacks
- Theoretical vulnerabilities without a working proof of concept

Private runtime internals (`olyx-core`), outbound agent internals (`olyx-edge`), cloud-side signing systems, and customer infrastructure are handled privately, but vulnerabilities affecting those areas should still be reported through the same contact.

## Sensitive Material

- Do not post exploit details in public issues, pull requests, or discussions.
- Do not include live credentials, customer payloads, or private runtime artifacts in reports.
- If you need to share sensitive reproduction material, say so in the initial email and we will coordinate a safer handoff.
