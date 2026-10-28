# Provider setup

Olyx has one built-in wire format: OpenAI-compatible HTTP. Use OpenAI directly,
Hugging Face's compatible router for models in its catalog, or a local compatible
server. Independent [provider plugins](plugins.md) can translate requests to
other services. The core does not bundle native service SDKs or adapters.

| Connection | Configuration | Credentials |
| --- | --- | --- |
| OpenAI | protocol `openai`, `https://api.openai.com/v1` | OPENAI_API_KEY |
| Hugging Face router | protocol `openai`, `https://router.huggingface.co/v1` | HF_TOKEN |
| Local compatible server | protocol `openai`, loopback base URL | Optional |
| External executable | protocol `plugin`, explicit command and capabilities | Adapter-specific; optional |

## OpenAI

```sh
oy init --provider openai
oy check olyx.json
oy route --input request.json
```

Set OPENAI_API_KEY before routing. Initialization writes aliases and a sample
request without saving credentials. Choose a model available to your account.

## Hugging Face

```sh
oy init --provider huggingface --model YOUR_HF_MODEL_ID
oy check olyx.json
oy route --input request.json
```

Set HF_TOKEN with permission to make Inference Providers calls. Choose a served
model from the Hugging Face catalog, not an arbitrary model repository. A provider
suffix in the Hugging Face model identifier, when supported, is preserved.
Olyx sends Chat Completions JSON to the router; Hugging Face handles its downstream
provider selection and translation. Only models/features available through that
service are accessible. This does not imply access to Bedrock, Claude, or every
model offered directly by a service. Availability, tools, images, token controls,
and streaming depend on the selected routed model.

This route shares the compatible HTTP transport with OpenAI. Its contract is
tested locally; live verification requires an HF_TOKEN and available model.
Hugging Face is an optional endpoint, not an Olyx dependency.

## Local models without accounts

Start your compatible model server, then use `examples/ollama.json`, adjusting
the model to one installed locally:

```sh
oy check examples/ollama.json
oy route --config examples/ollama.json --model local --input examples/request.json
```

The example uses `http://127.0.0.1:11434/v1`. Credential configuration may be
omitted for loopback HTTP endpoints; remote endpoints require a configured
credential variable. Olyx does not install or download local model weights.
Use your model server's instructions to start it and obtain a model.

## Fallback and streaming

`examples/fallbacks.json` tries OpenAI and then Hugging Face for eligible failures.
Set OPENAI_API_KEY and HF_TOKEN and replace the Hugging Face model placeholder.
Only connection failures, 429, and 5xx responses trigger HTTP fallback. Invalid
requests or credentials stop execution, and partial output never triggers a new
provider. The model ID sent to Hugging Face is recorded as configured; provider
selection internal to the router is not an Olyx provider attempt.

```sh
oy route --config examples/fallbacks.json --input examples/request.json
oy replay TRACE_ID --config examples/fallbacks.json --compare fast,huggingface
oy debug TRACE_ID --config examples/fallbacks.json
```

Set `stream: true` for cumulative CLI snapshots or HTTP SSE through serve.
Tools, image content, and supported generation options use standard Chat
Completions fields directly. There is no native_request escape hatch. Embeddings
require an OpenAI-compatible `/embeddings` endpoint or an embedding-capable plugin;
Hugging Face chat routing does not establish embeddings compatibility.

## Extending Olyx

Use an OpenAI-compatible endpoint or a provider plugin for another model engine
or service. A native provider API needs an adapter that translates its request
and response format.

Adapter authors can develop and run the [versioned plugin contract kit](plugins.md)
without provider accounts. [Verification status and tests](provider-testing.md)
keep local contract coverage separate from live service compatibility.

## References

- [Hugging Face Chat Completion](https://huggingface.co/docs/inference-providers/tasks/chat-completion)
- [Hugging Face Inference Providers](https://huggingface.co/docs/inference-providers/index)
