# Connect an application

Use `oy serve` as your application's model endpoint. It uses the same aliases,
ordered fallbacks, provider adapters, streaming, and SQLite traces as `oy route`.
The server runs on your infrastructure and stores traces in bundled SQLite.

## Start locally

Set your provider credential, then initialize a configuration:

```sh
export OPENAI_API_KEY=your-provider-key
oy init --provider openai
oy serve
```

`init` writes `olyx.json` and `request.json` without overwriting existing files.
Keys stay in environment variables. With one recognized provider credential,
`oy init` selects that provider automatically. Use `--provider`, `--model`, and
`--base-url` for another provider or endpoint. Some providers require an explicit
model identifier. Run `oy help init` for syntax.

Use `oy serve --local` to enforce loopback-only binding.

The server defaults to `127.0.0.1:8787`. Configure an application access token with
`OLYX_SERVER_API_KEY`; it is required when binding to a non-loopback address.
This token is separate from upstream provider credentials.

```sh
export OLYX_SERVER_API_KEY=your-application-access-token
oy serve --config olyx.json --host 127.0.0.1 --port 8787
```

## Configure the application

Point a Chat Completions client at `http://127.0.0.1:8787/v1` and use a configured
model alias. For example, in an application that already uses the Python OpenAI client:

```python
import os
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key=os.environ["OLYX_SERVER_API_KEY"],
)
response = client.chat.completions.create(
    model="fast",
    messages=[{"role": "user", "content": "Hello"}],
)
print(response.choices[0].message.content)
```

Use `stream=True` for incremental responses. `stream_options={"include_usage": True}`
adds a final usage chunk. Tool calls are emitted as incremental tool argument deltas.
Options must be supported by the compatible endpoint or declared plugin capabilities. See the [plugin guide](plugins.md). The server implements Chat Completions and embeddings; see [Embeddings and RAG](rag.md).
Responses, files, audio, and assistants APIs are not exposed.

For applications configured with `OPENAI_BASE_URL` and `OPENAI_API_KEY`, set those
variables **in the application process** to the Olyx URL and application access
token. Keep the real `OPENAI_API_KEY` in the Olyx server process.

## Run with Docker Compose

From a checkout of the public CLI repository:

```sh
export OPENAI_API_KEY=your-provider-key
export OLYX_SERVER_API_KEY=your-application-access-token
docker compose build
docker compose up -d
```

Compose builds the image locally, runs as UID 10001, binds the host port to loopback,
and persists traces in the `olyx-data` volume. There is no published image required.
The default alias `fast` uses OpenAI; mount your own configuration at
`/etc/olyx/olyx.json` and supply its provider credential variables for another route.
Keep `database` set to `/data/traces.db` for the bundled volume layout.
An application on the same Compose network uses `http://oy:8787/v1`.
Applications outside that network use the host's mapped endpoint.

```sh
docker compose exec oy oy debug --config /etc/olyx/olyx.json
docker compose exec oy oy replay TRACE_ID --config /etc/olyx/olyx.json
```

Replays make new provider calls. Back up the data volume; removing it removes traces.
Use a private network or TLS reverse proxy when exposing the server beyond localhost.
The server's bearer token authenticates clients; it does not provide per-user accounts.

## Endpoint and operational reference

| Endpoint | Behavior | Authentication |
| --- | --- | --- |
| `POST /v1/chat/completions` | JSON completion or SSE when `stream: true` | Bearer token when configured |
| `POST /v1/embeddings` | Document/query vectors for application-owned indexing and retrieval | Bearer token when configured |
| `GET /v1/models` | Configured model aliases | Bearer token when configured |
| `GET /healthz` | Process liveness | None |
| `GET /readyz` | Local SQLite availability; no upstream request | None |

Successful completion responses include `X-Olyx-Trace-Id`. Use it with
`oy debug TRACE_ID --config olyx.json` or `oy replay TRACE_ID` against the same
database. Debug and replay are CLI commands; they are not exposed as HTTP endpoints.

`--max-concurrency N` defaults to 32. Excess requests receive HTTP 429 `server_busy`.
Request size and output token limits use the existing configuration policy. Invalid
JSON and unknown aliases fail before a provider call. A provider failure before the
first streamed output returns a JSON error; a later failure emits an SSE error and
closes without `[DONE]`. Clients should treat that response as incomplete.

Configuration and provider credentials load at startup; restart after changing them.
SIGINT and SIGTERM stop accepting connections and wait for active requests. Set the
container stop grace period to accommodate your providers' response times. SQLite
is intended for one deployment instance with durable local storage; independent
replicas have independent traces. Do not share one database over a network filesystem.

## Cloud independence

Self-hosted serving makes no Olyx identity-service calls. Missing, expired, or revoked
Cloud credentials do not interrupt inference. Application traffic uses
`OLYX_SERVER_API_KEY`; upstream inference uses provider credentials.

Traces remain in your configured SQLite database. Platform sign-in and Cloud
synchronization are not included in this release. Managed deployment remains
separate from the public runtime.
