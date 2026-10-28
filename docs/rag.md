# Embeddings and RAG

Olyx exposes `POST /v1/embeddings` alongside `POST /v1/chat/completions`.
Your application owns document ingestion, chunking, vector storage, retrieval,
and context assembly. Olyx handles provider requests, configured replicas, and
local traces. It does not create a vector database or upload documents to a cloud
knowledge base.

## Configure models

Use `examples/rag.json` or add an embedding alias to your configuration:

```json
{
  "models": {
    "embed": [{
      "base_url": "https://api.openai.com/v1",
      "model": "text-embedding-3-small",
      "api_key_env": "OPENAI_API_KEY"
    }]
  }
}
```

The endpoint supports OpenAI-compatible embedding endpoints and plugins declaring
`embeddings`. The application owns ingestion, indexing, and retrieval. Hugging
Face chat routing alone does not establish compatibility with `/embeddings`.

```sh
export OPENAI_API_KEY=your-provider-key
export OLYX_SERVER_API_KEY=your-application-access-token
oy check examples/rag.json
oy serve --config examples/rag.json
```

For local experimentation and CI, add `--local` to the serve command to enforce
loopback-only binding:

```sh
oy serve --local --config examples/rag.json
```

`OLYX_SERVER_API_KEY` authenticates applications to your server. It is separate
from the upstream provider key.

## Generate document and query vectors

```sh
curl http://127.0.0.1:8787/v1/embeddings \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $OLYX_SERVER_API_KEY" \
  -d '{"model":"embed","input":["First document chunk","Second document chunk"],"encoding_format":"float"}'
```

The response uses the OpenAI embeddings shape: `object: "list"`, `data` entries
with `object`, `index`, and `embedding`, the provider model, and provider usage.
Entries are returned in input order. `X-Olyx-Trace-Id` identifies the saved trace.
The endpoint accepts a text string, a batch of strings, one token-ID array, or a
batch of token-ID arrays. Input types cannot be mixed and empty inputs are rejected.

`encoding_format` accepts `float` (default) or `base64`. Base64 contains packed
little-endian float32 vectors. Optional positive `dimensions` is forwarded to the
provider and validated against the returned vectors. Provider model token limits
and supported dimensions still apply; Olyx does not silently truncate inputs.

For an application already using the Python OpenAI client:

```python
import os
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key=os.environ["OLYX_SERVER_API_KEY"],
)
result = client.embeddings.create(
    model="embed",
    input=["A document chunk", "Another document chunk"],
    encoding_format="float",
)
vectors = [item.embedding for item in result.data]
```

Persist vectors with their source text, document identifiers, actual model version,
and dimensions in your application’s chosen index. Embed each query with the same
model and settings, retrieve matching chunks, then send those chunks and the query
to `/v1/chat/completions`. Updating the embedding model requires rebuilding the index.
Model aliases alone do not guarantee stable model versions; pin versions where your
provider supports them.

## Run the application-owned example

The repository includes `examples/rag.py`, a dependency-free Python example. It
reads UTF-8 `.txt` and `.md` files, chunks them, batches embedding requests, and stores
a local JSON index owned by the application. Queries use cosine similarity to select
three chunks before generating an answer with source references.

With Olyx serving `examples/rag.json` in another terminal:

```sh
python3 examples/rag.py index ./documents --index rag-index.json
python3 examples/rag.py ask 'What does the documentation say about backups?' --index rag-index.json
```

The example checks that document and query vectors use the same reported model and
dimensions. The JSON index and linear search demonstrate the integration; substitute
your application's vector store for larger collections. PDF parsing, access control,
document updates, deletion, deduplication, and retrieval quality belong to the application.
Source references are supplied to the model; Olyx does not verify generated citations.

## Fallbacks, traces, and limits

Embedding fallback candidates must use the same configured model identifier and
protocol. Different embedding models can produce incompatible vector spaces even
when dimensions match, so a mismatched route fails before making a provider call.
Ensure configured replicas actually serve the same model/version and settings.
Transport failures, HTTP 429, and 5xx try the next replica. Authentication and
invalid-request errors stop execution. A malformed successful response also stops
execution; it does not trigger another provider call.

Request size uses `policy.max_body_bytes` (default 256 KiB). Embedding responses use
`policy.max_embedding_response_bytes` (default 64 MiB; configurable up to 1 GiB).
The existing `--max-concurrency` pool covers embeddings and chat together. Chat's
`max_output_tokens` setting is not added to embedding requests.

Traces retain the input and summary metadata: embedding count, dimensions, encoding,
usage, latency, and configured cost telemetry. Vector values are returned to the
application and are not duplicated into SQLite. Debug and replay work with these
traces. Replay makes new embedding calls and returns trace summaries, not vectors;
call the HTTP endpoint when you need vectors for an index. Comparison reissues inputs
against explicitly selected aliases; it does not score retrieval or compare vectors.

## Embedding response model identity

An embedding response must report its model identifier. By default it must match
the configured provider `model`; otherwise Olyx returns `embedding_model_mismatch`
and saves a failed attempt. It does not silently relabel vectors or fall back
after receiving the response.

For a deployment name or a provider that reports an equivalent versioned ID, list
the additional identifiers explicitly on the provider:

```json
{
  "model": "embedding-deployment",
  "base_url": "https://example.com/openai/v1",
  "protocol": "openai",
  "api_key_env": "EMBEDDING_API_KEY",
  "embedding_response_models": ["text-embedding-3-small"]
}
```

Only approve identifiers you know represent the same embedding space. The actual
response ID is preserved for your application's index metadata. Every replica in
an embedding fallback route must have the same configured model, protocol, and
set of approved response identifiers. Changing model versions may require
re-indexing application documents even when their dimensions are unchanged.
