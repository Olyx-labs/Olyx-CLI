# olyx-cli

Command-line client for the Olyx CLI/agent distribution channel.

The CLI is the regular-user entry point for local execution. Enterprise
deployments pair it with `olyx-edge`, which runs as the outbound agent inside the
customer environment.

## Commands

```sh
olyx config
olyx doctor
olyx trace --metadata '{"source":"cli"}'
olyx trace --local-ref opaque:trace_01
olyx replay trace_01
```

Configuration comes from flags or environment variables:

```sh
export OLYX_BASE_URL=http://localhost:3000
export OLYX_API_KEY=ak_...
```

The CLI uses the same Rust HTTP stack as `olyx-edge`, so both local
`http://localhost:3000` and production `https://app.olyxai.io` calls are
supported.

## Distribution Boundary

The CLI must not collect or upload provider secrets. Provider keys, private model
tokens, prompts, completions, and customer trace payloads stay in the customer
environment. Olyx Cloud receives policy/config requests, trace metadata, opaque
local references, and replay coordination requests.
