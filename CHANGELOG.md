# Changelog

## 0.2.0 — prepared for release

- Local `route` with model aliases and ordered provider fallbacks.
- Durable SQLite traces with configurable retention, latency, and cost estimates.
- Live replay/comparison that preserves the source trace, and `debug` inspection.
- Local commands work without an Olyx account or enterprise license.
- Routing and replay do not scan or redact prompt/response content.
- Cloud replay model overrides and optional job polling.
- Standalone Apache-2.0 public workspace without private runtime dependencies.
- Example configuration and requests, licensing, and documentation in binary archives.
- Patched dependency advisories and owner-only Unix trace database/journal files.

- Added native Anthropic and Gemini adapters, SSE streaming with live NDJSON snapshots, and streamed trace replay.

## 0.1.0

Initial control-plane CLI: inspect, login, config, trace lifecycle, and Cloud replay.

