# Olyx CLI

Olyx routes requests to AI models and records what happened in a local SQLite
database. Send a request, inspect its trace, and replay it against another
configured model. Use Olyx from your terminal or through an OpenAI-compatible
HTTP server.

Connect OpenAI, Hugging Face, a local model server, or an independent provider
plugin. SQLite is bundled; a database server is not required.

## Installation

[Download Olyx 0.2.1 from GitHub Releases](https://github.com/Olyx-labs/Olyx-CLI/releases/tag/v0.2.1).
Choose the archive for macOS Apple Silicon, Linux x64, or Linux ARM64. Extract
it and put the `oy` executable in a directory on your `PATH`. No Node.js or Rust
installation is needed for these binaries.

Follow the [installation guide](https://docs.olyxai.io/getting-started/installation/)
for platform-specific setup, checksum verification, and building from source.
Check the installed version:

```sh
oy --version
```

```text
oy 0.2.1
```

With Node.js 22 or newer, you can also install the matching npm release:

```sh
npm install -g olyx-cli@0.2.1
```

To run without a global installation, use `npx olyx-cli@0.2.1 --help`.
External plugins may require their own language runtime.

## Quickstart

This walkthrough uses an OpenAI API key with access to `gpt-4o-mini` and
sufficient API credit. Requests and replays make provider calls and may incur
charges. For a local model or another provider, see [provider setup](docs/providers.md).

### 1. Create a project

Replace `YOUR_OPENAI_API_KEY` with your provider key, then run:

```sh
mkdir olyx-example
cd olyx-example
export OPENAI_API_KEY="YOUR_OPENAI_API_KEY"
oy init --provider openai
```

`init` creates `olyx.json` and `request.json`. It reads credentials from your
environment without saving them to either file. An existing configuration stops
initialization; an existing request file is kept.

The configuration names the model `fast`. This alias lets you change the
underlying provider or model without changing your requests. The generated
`request.json` contains:

```json
{
  "model": "fast",
  "messages": [
    {"role": "user", "content": "Hello!"}
  ]
}
```

### 2. Send a request

Olyx records prompts, responses, and provider attempts in `data/traces.db`,
relative to the configuration file. Keep this database private. See
[storage and backups](docs/storage.md) for retention and backup options.

```sh
oy check olyx.json
oy route --input request.json
```

`check` validates the configuration without calling a provider. `route` sends
the request and returns JSON. Here is a shortened example; your trace ID and
model response will differ:

```json
{
  "trace_id": "trace_...",
  "output": {
    "choices": [
      {"message": {"role": "assistant", "content": "Hello! How can I help?"}}
    ]
  }
}
```

If setup fails, run `oy doctor --config olyx.json`. It checks configuration,
required credentials, and storage access without making inference calls.
See [troubleshooting](docs/diagnostics.md) for interpreting failures.

### 3. Inspect and replay

Replace `TRACE_ID` with the ID returned by `route`:

```sh
oy debug TRACE_ID
oy replay TRACE_ID
```

`debug` shows the recorded request, provider attempts, response, and timing.
`replay` makes a new provider call and preserves the original trace. See the
[command reference](https://docs.olyxai.io/reference/cli/) for comparing models.

## Optional workflows

### Configure fallback providers

An alias can list providers in the order Olyx should try them. Connection
failures, rate limits, and provider server errors can trigger fallback.
Invalid requests and authentication errors stop the request. For HTTP
providers, Olyx does not fall back after a successful HTTP response, including
when its content cannot be decoded or a stream ends early.

See [providers and local models](docs/providers.md) for configuration examples,
including Hugging Face and account-free local endpoints.

### Stream a response

Save this as `streaming-request.json`:

```json
{
  "model": "fast",
  "messages": [{"role": "user", "content": "Explain ordered fallbacks briefly."}],
  "stream": true
}
```

```sh
oy route --input streaming-request.json
```

Olyx prints updates as JSON lines, followed by a completed event with the trace
ID. Pass `--config FILE` or set `OLYX_CONFIG` to use another configuration.

### Connect an application

`oy serve` exposes OpenAI-compatible Chat Completions, embeddings, and model-list
endpoints. Your application still runs tools, loads documents, and searches data.

Choose an application secret, distinct from your provider key, and replace
`YOUR_APPLICATION_SECRET` below:

```sh
export OLYX_SERVER_API_KEY="YOUR_APPLICATION_SECRET"
oy serve
```

Keep this terminal running. Point your client at `http://127.0.0.1:8787/v1`,
use `fast` as its model, and use the application secret as its API key. The
server accepts connections from the same machine by default.

See [application integration](docs/deployment.md) for a complete client example,
remote serving, Docker, and authentication. Non-loopback listeners require
`OLYX_SERVER_API_KEY`.

## Documentation

- [Quickstart guide](https://docs.olyxai.io/getting-started/quickstart/)
- [Command reference](https://docs.olyxai.io/reference/cli/)
- [Provider plugins](docs/plugins.md)
- [Embeddings and RAG](docs/rag.md)
- [Storage and backups](docs/storage.md)
- [Troubleshooting](docs/diagnostics.md)

Run `oy help` or `oy help COMMAND` for command-specific options.

## Contributing

Issues and documentation feedback are welcome. External code contributions are
closed during beta; see the [contributing guide](CONTRIBUTING.md) for the current
policy and development setup. From a source checkout, run:

```sh
cargo test --locked --workspace
```

Report vulnerabilities through the [security policy](SECURITY.md).
Maintainers can follow the [release guide](RELEASING.md).

## License

Olyx CLI and its runtime are released under [Apache License 2.0](LICENSE).
Independent plugins use their authors' chosen licenses.
