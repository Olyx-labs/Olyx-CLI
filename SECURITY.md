# Security Policy

## Supported Versions

| Version | Supported |
| :--- | :--- |
| 0.2.x (prepared release) | Development only; not yet published |

No public release is available yet. Once published, only the latest release will
receive security fixes. Include the affected version or source commit when reporting.

## Reporting a Vulnerability

**Do not open a public GitHub issue for a suspected vulnerability.**

Report security issues privately to `security@olyxai.io` with:

- A short summary of the issue
- Affected version (`oy --version`) and operating system
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

This repository covers routing, replay/comparison, trace debugging, HTTP serving,
provider plugins, and release packaging.

**In scope:**
- Credential leakage through CLI output, error messages, logs, or process arguments
- Provider credential forwarding, local trace file permissions, and replay isolation
- Supply chain issues in published release artifacts (checksum, Sigstore bundle, GitHub Actions workflow)
- Privilege escalation or arbitrary code execution triggered by CLI input

**Out of scope:**
- Vulnerabilities in dependencies already publicly disclosed and tracked by cargo-deny
- Social engineering or phishing attacks
- Theoretical vulnerabilities without a working proof of concept

For an issue affecting another Olyx project, use the same private reporting
contact.

## Sensitive Material

- Do not post exploit details in public issues, pull requests, or discussions.
- Do not include live credentials, customer payloads, or private runtime artifacts in reports.
- If you need to share sensitive reproduction material, say so in the initial email and we will coordinate a safer handoff.
