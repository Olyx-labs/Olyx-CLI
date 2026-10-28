# Provider plugins

Olyx owns aliases, ordered fallback, request limits, HTTP serving, trace storage,
replay, and diagnostics. Its built-in transport speaks OpenAI-compatible HTTP.
A provider plugin is an explicitly configured local executable that translates
the same requests to another service, SDK, or local engine. Configure its command
and capabilities in your local configuration.

The core no longer includes native Anthropic, Gemini, Bedrock, Azure, Vertex,
Cohere, or Responses adapters. Existing native protocol configurations must be
migrated to a compatible HTTP endpoint or a plugin. Native APIs are extension
work, not claimed built-in support. Hugging Face is an optional compatible
endpoint and is not required by the core.

## Configure an adapter

```json
{
  "models": {
    "custom": [{
      "protocol": "plugin",
      "model": "my-model",
      "api_key_env": "MY_PROVIDER_KEY",
      "plugin": {
        "command": "/absolute/path/to/adapter",
        "args": [],
        "capabilities": ["chat", "streaming", "tools", "embeddings"],
        "env": [],
        "timeout_ms": 60000
      }
    }]
  }
}
```

`command` must be absolute. Arguments are passed directly; no shell is used.
For a Python adapter, set command to your interpreter and pass the absolute
script path in args. Omit api_key_env for an account-free local adapter. Do not
set base_url for a plugin. List only capabilities the adapter implements.
An unsupported requested capability fails before launching the executable.
Embedding replicas must retain the same model identity, protocol, and approved
response identifiers, just as with HTTP providers.

Use ordinary route, serve, debug, replay, and comparison commands. Plugins are
also invoked during replay, which may make new provider calls. The CLI returns
tool calls; your application executes tools.

## Version 1 wire contract

One subprocess handles one invocation. It reads one newline-terminated JSON
object from stdin, then stdin closes:

```json
{"protocol_version":1,"operation":"chat","model":"my-model","request":{"model":"my-model","messages":[{"role":"user","content":"Hello"}],"max_tokens":128}}
```

Operation is `chat` or `embeddings`. Request fields use the common OpenAI schema;
model is already resolved from the alias. Respect stream and token controls.
Translate fields you support and reject unsupported options rather than dropping
them. The core applies its normal body/output limits and rejects native_request.

Stdout is exclusively newline-terminated JSON events. First send:

```json
{"type":"hello","protocol_version":1,"capabilities":["chat","streaming","tools","embeddings"]}
```

Runtime capabilities must cover the operation requested and configured. For
nonstreamed chat, send a normalized result:

```json
{"type":"result","output":{"choices":[{"index":0,"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}}
```

For streaming chat, emit cumulative `snapshot` events using the same output
shape, followed by `result`. The last snapshot must equal the final result;
include final finish reasons and usage before completing. Each snapshot is
validated and sent to the CLI/HTTP client. Do not emit OpenAI SSE on plugin
stdout: Olyx creates the client-facing SSE or NDJSON envelope.

For embeddings, send one result with the standard model/data/usage shape:

```json
{"type":"result","output":{"model":"my-model","data":[{"index":0,"embedding":[0.1,0.2]}],"usage":{"prompt_tokens":2,"total_tokens":2}}}
```

The core validates vector dimensions, ordering, finite values, batch size,
usage, and model identity. Embedding traces store summaries; replay reexecutes
the recorded input. Match requested dimensions and encoding_format.

For failure, send an error after hello:

```json
{"type":"error","code":"unavailable"}
```

Codes are unavailable, rate_limited, authentication_failed, or invalid_request.
Unknown codes become plugin_failed. Do not put provider messages or credentials
on stdout. Only unavailable/rate_limited permit ordered fallback, and only before
a chat snapshot was emitted. A timeout, malformed event, version mismatch,
nonzero exit, missing result, result mismatch, or extra event after result fails
the attempt. Core traces retain the stable error and any partial chat output.

The executable must exit successfully after its result. Each event is limited
to 1 MiB; total chat output is limited to 8 MiB. Embeddings use the configured
max_embedding_response_bytes total limit with the same per-event limit. Default
timeout is 60 seconds; configure 100–300000 ms. Cancellation/timeout drops the
child with kill-on-drop; plugins must manage their own descendant processes.

## Execution and credentials

Installed plugins are trusted executable code, not sandboxed extensions.
Olyx does not install or discover executables automatically. A child receives
PATH, system/temp variables, the selected api_key_env value, and only additional
variables explicitly listed in plugin.env. Unrelated provider tokens and Olyx
login credentials are not inherited. Credentials never travel in RPC JSON.
Plugin stderr is discarded to avoid leaking raw provider errors into output.
Version 1 diagnostics use fixed codes; it provides no automatic credential
refresh, plugin signing, sandboxing, or upstream certification.

## Account-free development and tests

`examples/plugins/echo.py` is a runnable adapter fixture. It echoes text, returns
synthetic vectors and tool calls, and never contacts a provider. It is a test
fixture, not a model. Use it as a starting point for a fixture mode in a real
adapter, keeping SDK calls behind a separately tested translation layer.

```sh
./scripts/test-providers
python3 scripts/smoke-plugins.py target/debug/oy
```

The core contract smoke checks CLI and HTTP chat, streaming, tools, embedding
batches, trace persistence, replay immutability, capabilities, version errors,
malformed output, timeouts, child failures, environment isolation, eligible
fallback, and no fallback after partial output. It requires no provider account.

Test an independently developed adapter using its fixture configuration:

```sh
./scripts/test-plugin --config /path/to/plugin-fixture.json --alias custom \
  --report plugin-contract.json
```

The kit tests declared chat/streaming/tools/embeddings capabilities through real
CLI/HTTP paths. It launches the configured executable; use your adapter's fixture
mode to avoid network calls and provider charges. Its sanitized report establishes
plugin contract behavior, not real service compatibility. Live provider testing
can be performed independently by the adapter's maintainer or a contributor.
