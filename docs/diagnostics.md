# Diagnose a request

## Check local setup

```sh
oy doctor --config olyx.json
oy doctor --probe --config olyx.json
```

`doctor` checks configuration, credential environment-variable presence, and
SQLite storage. It prints JSON and exits 1 when any check fails, 0 otherwise.
Default checks make no network requests. Storage
validation opens an existing database read-only and writes/removes an empty
probe file in its nearest existing parent directory; it does not create a
trace database, migrate schema, or expire traces.

`--probe` additionally sends one unauthenticated HEAD request per distinct
provider base URL, with a five-second timeout per endpoint and no redirects.
Any HTTP response proves connectivity only: even 401, 404, or 405 does not
validate credentials, model access, or inference readiness. No keys or inference
content are sent. A successful check is not a production-readiness guarantee.

A running gateway loads provider credentials at startup. After updating a key,
restart the gateway. Running doctor in another shell checks that shell's
configuration and environment; it cannot inspect a running process's keys.

## Read structured errors

Provider failures retain HTTP 502 to distinguish them from gateway bearer-token
failures (401). The body includes stable `error.code`, a separate `trace_id`,
and, for provider HTTP/transport failures, sanitized diagnostic fields:

```json
{
  "error": {
    "code": "provider_authentication_failed",
    "message": "The provider rejected the configured credentials or permissions.",
    "type": "server_error",
    "param": null,
    "trace_id": "trace_123",
    "provider": "api.openai.com",
    "alias": "fast",
    "upstream_status": 401,
    "retryable": false,
    "fallback": "stopped_non_retryable",
    "hint": "Check the configured provider credential and permissions. Running gateways load credentials at startup; restart after updating them."
  }
}
```

`X-Olyx-Trace-Id` also identifies traced error responses. Codes no longer contain
trace IDs. Raw upstream response bodies, credentials, and authorization headers
are not copied into diagnostics. The provider field is the endpoint hostname.

| Code | Meaning | Fallback |
| --- | --- | --- |
| `provider_authentication_failed` | Upstream 401 or 403 | Stops |
| `provider_request_rejected` | Other nonretryable upstream rejection | Stops |
| `provider_rate_limited` | Upstream 429 | Tries remaining candidates |
| `provider_unavailable` | Upstream 5xx | Tries remaining candidates |
| `provider_transport_failure` | Connection, DNS, TLS, or transport failure | Tries remaining candidates |
| `provider_credentials_unavailable` | Configured credential absent at startup | Stops |

A returned retryable provider error reports `fallback: exhausted`: configured
candidates have already been attempted. `retryable` describes whether a later
new request may be worth attempting, not an instruction to automatically retry.
Never fall back after a successful upstream response has begun: partial streams
may have delivered output or incurred usage. Stream failures use an SSE error
event with a stable code and trace ID when available.

## Inspect a failed trace

```sh
oy debug TRACE_ID --config olyx.json
```

Debug prints a failure summary and hint to stderr before the full trace JSON on
stdout. The JSON includes a top-level `diagnostic` for failed traces. Existing
traces without structured metadata remain readable and direct you to their
recorded provider attempts. Use the same configuration file as the original
request to find its SQLite database. Debug does not require provider credentials.

`/readyz` checks local SQLite access; its response explicitly reports provider
connectivity as not checked and credentials as not validated.

## Successful HTTP responses that fail validation

A provider HTTP 200 does not establish a valid completion. Missing or malformed
Chat Completions messages return `invalid_provider_response`. For OpenAI-compatible
streams, both `[DONE]` and a nonempty finish reason for every accumulated choice
are required; otherwise the request fails with `provider_stream_incomplete`.
Partial output remains in the failed trace. These failures do not trigger fallback
after a response has begun, and the gateway does not emit a successful `[DONE]`.

Missing credentials and local provider-request validation failures record an
attempt with `request_sent: false`. This distinguishes requests stopped locally
from requests that reached an upstream provider.

Native adapters report `usage.total_tokens` when token counts are available.
Provider-reported totals are preserved where supplied; otherwise the total is the
sum of reported input and output tokens. Missing usage remains unknown rather
than becoming zero. This counts individual provider calls; applications must
aggregate all planner, tool, and embedding calls for a complete workflow total.

## Provider contract and cancellation failures

Olyx rejects malformed tool-call fields, response roles/content blocks, duplicate
choice indices, error envelopes disguised as completions, and invalid token
metadata. `invalid_provider_usage` means a counter was negative, fractional,
malformed, or overflowed; absent usage remains unknown. Reported totals are
preserved in HTTP and SSE responses. Plugins report normalized usage; missing
counters are not invented.

A truncated tool call retains `finish_reason: "length"`; do not execute its
incomplete arguments. Plugins must normalize upstream filtering to `content_filter`.
Valid streamed tool arguments may be partial
until their final event. Olyx checks the wire types rather than treating a partial
argument string as executable JSON.

A dropped request future records `request_cancelled` on its pending trace. This
covers client disconnections and task cancellation while the process remains
running. The provider may already have incurred usage. Abrupt process termination
cannot guarantee this asynchronous cleanup has run.

SSE decoding accepts a leading UTF-8 BOM, CR/LF/CRLF line endings, multiline data,
and fragmented UTF-8. Choice indices are retained in JSON and SSE.
The HTTP Bearer authentication scheme is case-insensitive; the token is exact.

## Comparisons with failed aliases

`oy replay TRACE_ID --compare fast,quality,other` attempts every configured alias,
even if one fails. The JSON includes all traces, `comparison.succeeded`, and
`comparison.failed`. Top-level status is `completed`, `partial`, or `failed`.
Failures retain diagnostics in each trace's metadata. Any failed comparison makes
the CLI exit nonzero after printing the JSON, so CI can detect partial failures
without losing successful results. Ordinary replay without `--compare` still
fails immediately on its execution error. Original traces are unchanged.

## Interrupting a local request

Press Ctrl-C during `oy route` or `oy replay` to cancel the active request.
The CLI waits for pending trace cancellation to be saved before exiting with
status 130. Inspect the trace with `oy debug TRACE_ID`; its diagnostic code is
`request_cancelled`. Partial output does not count as a completed response.
Upstream usage may already have occurred. Forced termination such as SIGKILL
cannot perform this cleanup.
