# Changelog

## 0.2.0 — prepared for release

- Send `X-Content-Type-Options: nosniff` on HTTP responses.
- Authenticate application API requests before reading JSON bodies, rejecting
  unauthorized slow-body requests without consuming provider capacity.
- Route Chat Completions through configured model aliases and ordered fallbacks.
- Connect OpenAI-compatible HTTP endpoints, Hugging Face, local model servers,
  and independent provider plugins using the versioned subprocess interface.
- Stream responses, save local SQLite traces, inspect failures, and replay or
  compare requests without changing the original trace.
- Serve Chat Completions, embeddings, and model aliases over HTTP. Use an
  application bearer token for remote listeners; no Olyx account is required.
- Add configuration generation, setup diagnostics, structured errors, request
  validation, cancellation handling, and explicit embedding model identities.
- Include a non-root container, persistent storage examples, and a RAG example
  where the application owns document loading and retrieval.
- Distribute native executables with bundled SQLite, checksums, and signed
  release artifacts. Users do not need Rust or a database server.
- Add account-free provider tests and opt-in live verification. Local test
  results do not establish compatibility with every hosted model service.
- Remove platform login, Cloud commands, enterprise activation, and built-in
  native provider translators from the public CLI.
- Update Tokio to 1.54.1 to address RUSTSEC-2026-0386.
- Add local `doctor` checks and opt-in credential-free connectivity probes.
- Return stable provider error codes with separate trace IDs, upstream status,
  fallback reason, and recovery hints.
- Persist sanitized diagnostics and lead failed-trace debug output with a summary.
- Clarify readiness checks do not validate provider credentials.
- Recover interrupted traces when the database reopens without changing active
  requests or resending provider calls.
- Save failure details atomically and report corrupt stored JSON explicitly.

## 0.1.0

Initial control-plane CLI: inspect, login, config, trace lifecycle, and Cloud replay.
