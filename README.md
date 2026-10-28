# Olyx CLI

## What is Olyx?

Olyx is a command-line tool for working with AI models. Send a request, inspect
what happened, and run it again against another model. Use it in your terminal
or as an HTTP endpoint for your application.

Connect Hugging Face, OpenAI, a local model server, or your own provider plugin.
Olyx stores requests, responses, and provider attempts in a local SQLite database.

## How it works

### Routing

Give a model a name, such as `fast`, in your configuration. This name is an
alias: your requests use it instead of a provider-specific model name. Each
alias can list several providers, which Olyx tries in order.

Connection failures, rate limits, and provider server errors can trigger the
next provider. Invalid requests and authentication errors stop the request.
Once streaming output begins, Olyx does not switch providers.

### Traces and replay

Each request gets a trace ID. Use `oy debug` to inspect its inputs, responses,
providers tried, and time taken. Use `oy replay` to send the recorded request
again or compare configured models. Replay makes new provider calls and keeps
the original trace.

### Use with an application

`oy serve` runs an HTTP server with OpenAI-compatible Chat Completions,
embeddings, and model-list endpoints. It uses the same configuration and local
trace database as the terminal commands. Your application still runs tools,
loads documents, and searches its own data.

## Getting started

### 1. Run Olyx

With Node.js 22 or newer:

```sh
npx olyx-cli --help
```

npm installs the matching executable for macOS Apple Silicon or Linux x64/arm64. Rust and a database server are not required; SQLite is bundled.
External plugins may have their own runtime requirements.

To use the shorter `oy` command throughout this guide:

```sh
npm install -g olyx-cli
```

Or replace `oy` in each example with `npx olyx-cli`. For a source checkout, see the
[contributor installation guide](https://docs.olyxai.io/getting-started/installation/#build-from-source-contributors).

### 2. Connect a model

In a new project directory, run one of the following examples.

For Hugging Face, select a model available through its Inference Providers router:

```sh
export HF_TOKEN=your-hugging-face-token
oy init --provider huggingface --model YOUR_HF_MODEL_ID
```

Or use OpenAI:

```sh
export OPENAI_API_KEY=your-provider-key
oy init --provider openai
```

`init` creates `olyx.json` and a sample `request.json` in the same directory.
The configuration defines a `fast` alias and reads your API key from the
environment; the key is not saved to either file. An existing configuration
stops initialization. An existing request file is kept.
For a local model or another endpoint, see [provider setup](docs/providers.md).

### 3. Send a request

```sh
oy check olyx.json
oy route --input request.json
```

`check` validates the configuration without calling a provider. `route` sends
the request to the configured model and saves a trace. Its JSON response
contains `trace_id` and `output`. Replace `TRACE_ID` below
with the returned ID:

```sh
oy debug TRACE_ID
oy replay TRACE_ID
```

To stream a response, save this as `streaming-request.json`:

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

Olyx prints each update as a JSON line, followed by a completed event with the
trace ID. To use another configuration file, pass `--config FILE` or set
`OLYX_CONFIG`.

### 4. Connect an application

```sh
export OLYX_SERVER_API_KEY=your-application-secret
oy serve --local
```

Point your application's compatible client at `http://127.0.0.1:8787/v1`, use
`fast` as the model, and send the application secret as its API key. This is
separate from your provider key. `127.0.0.1` refers to the machine running
the server; `--local` only accepts connections from that machine.

For example, keep the server running and send a request from another terminal:

```sh
export OLYX_SERVER_API_KEY=your-application-secret
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $OLYX_SERVER_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"fast","messages":[{"role":"user","content":"Hello!"}]}'
```

See [application integration](docs/deployment.md) for remote serving, Docker
Compose, authentication, and persistent storage. Non-loopback listeners require
`OLYX_SERVER_API_KEY`.

## Documentation

- [Getting started](https://docs.olyxai.io/getting-started/quickstart/)
- [Command reference](https://docs.olyxai.io/reference/cli/)
- [Providers and local models](docs/providers.md)
- [Provider plugins](docs/plugins.md)
- [Embeddings and RAG](docs/rag.md)
- [Storage and backups](docs/storage.md)
- [Troubleshooting](docs/diagnostics.md)

Run `oy help` for commands or `oy help COMMAND` for a specific command.
If setup fails, start with:

```sh
oy doctor --config olyx.json
```

This checks the configuration, required API-key variables, and access to local
storage. It does not call a model or verify your provider account.

Traces contain application inputs and outputs. Keep the database private and
review traces before sharing them. CLI output masks recognizable API keys and other credentials,
but can still contain sensitive application data.

## Contributing

See the [contributing guide](https://github.com/Olyx-labs/Olyx-CLI/blob/master/CONTRIBUTING.md)
for development setup. From a source checkout, run the account-free test suite:

```sh
./scripts/test-providers
```

These checks use local test servers and plugins with predictable responses.
They check how Olyx handles requests and failures; they do not establish live
access to every model provider. See [provider verification](docs/provider-testing.md)
for optional live checks.

Report vulnerabilities through the [security policy](https://github.com/Olyx-labs/Olyx-CLI/blob/master/SECURITY.md).
Release procedures are in the [maintainer guide](https://github.com/Olyx-labs/Olyx-CLI/blob/master/RELEASING.md).

## License

Olyx CLI and its runtime are released under [Apache License 2.0](LICENSE).
Independent plugins use their authors' chosen licenses.
