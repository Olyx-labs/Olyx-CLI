# Provider testing and verification

You can develop and test adapters without buying access to every provider.
The ordinary CI suite uses account-free local providers, synthetic response
fixtures, and adversarial streams. Live certification is a separate opt-in step.

## What the checks establish

“Contract-tested” means the adapter passes synthetic protocol tests. “Live-verified”
means specific checks passed against a real endpoint, with a particular model and
binary at a particular time. Neither certifies every model, region, account,
parameter, or future provider version. OpenAI-compatible services such as Groq
and Mistral share an adapter but still need their own live checks; a result
from another provider does not establish theirs.

## Account-free checks

Run these commands from a source checkout. The npm package does
not include the full development test suite.

```sh
./scripts/test-providers
```

This command builds the debug CLI, runs Rust unit tests, all eight integration
suites, and the Python verification tests, then prints a per-protocol coverage
summary. It requires Rust/Cargo, a C compiler, and Python 3, with no provider
accounts. The initial build may download dependencies. It never starts Docker.
Local suites receive a restricted environment without provider credentials.

Use an existing binary to skip the Cargo build and Rust unit tests:

```sh
python3 scripts/test-providers --binary target/release/oy --report provider-tests.json
```

The JSON report includes suite status and local contract coverage; failed suites
do not stop the remaining checks. A failure or timeout makes the command exit
nonzero. Report files must be new. Raw subprocess output and local paths are
omitted from the report and console summary.

You can combine local checks with explicit live certification:

```sh
./scripts/test-providers --live-config /path/to/private-provider-config.json \
  --alias huggingface --require-live --report provider-tests-live.json
```

Live checks run only after local checks pass. Without `--live-config` and
`--alias`, no live provider is called, even when credentials are present.
`--require-live` fails if a selected route is skipped. Live summaries and detailed
sanitized evidence appear in the combined report. Model support and charges
remain subject to the certification limits below.

Local HTTP fixtures cover compatible JSON/SSE and errors. Plugin fixtures cover
version negotiation, capability rejection, tools, streaming, embeddings, lifecycle,
credential isolation, trace/replay, and failure behavior. Native API translators
are not in the core and have no built-in service compatibility claim. Use the
[plugin conformance kit](plugins.md) to test an independently developed adapter.

Keep reusable synthetic fixtures in these suites when adding a regression. Do
not commit raw production traces or recordings: they may contain credentials,
customer data, model responses, or account identifiers. The certification report
is sanitized test evidence, not a wire-response recording.

## Optional live checks

Use a private configuration containing exactly one candidate per selected alias.
Copy the relevant route from `examples/providers.json`, replace its placeholders,
and select a model your account can invoke. Credentials stay in environment
variables referenced by `api_key_env`.

```sh
cargo build --locked --release --bin oy
python3 scripts/certify-providers.py \
  --config /path/to/private-provider-config.json \
  --alias huggingface \
  --report huggingface-certification.json \
  --require-live
```

Set `HF_TOKEN` in your shell for that example. Choose a served model available
to your Hugging Face account.
Other providers use the credentials described in [provider setup](providers.md).
Repeat `--alias` to opt in additional routes. Nothing is tested merely because
its credentials happen to be present. No provider accounts are provisioned.

Each selected route makes three inference calls: one brief chat, one streaming
chat, and one replay. Output is limited to 128 tokens per call and each command
has a 90-second timeout. Debug checks run offline between these calls. There are
no automatic retries or fallback routes. Provider charges still apply; this
bounds requests rather than guaranteeing a currency amount. Models requiring
more output tokens may fail this deliberately small smoke test.

A PASS verifies a nonempty, nontruncated response, streamed snapshots with a
completed event, a persisted completed trace, and a successful replay that leaves
the original trace unchanged. It does not measure answer quality, tools,
embeddings, cross-provider fallbacks, HTTP serving, or production throughput.
Those have separate local tests and, where available, application live audits.

Missing credentials, invalid endpoints, and aliases with fallback candidates are
SKIP. Failed executed checks produce a nonzero exit. `--require-live` also makes
any SKIP fail the run; omit it for a credential-optional developer run. An all-SKIP
report is never evidence of a successful certification. Reports must use a new
file path so an existing configuration or report cannot be overwritten.

Reports contain a timestamp, binary/suite fingerprints, protocol, numbered route,
check status, elapsed time, and fixed diagnostic categories. They exclude alias
names, endpoint URLs, deployment/model IDs, raw errors, trace IDs, inputs,
responses, and credentials. Numbered routes follow `--alias` order; maintainers
retain the private configuration separately to interpret the exact scope.
Temporary databases and traces are deleted when the runner exits normally.
Do not upload additional local files with the report.

`--allow-loopback` exists for harness development. It marks the whole report
`simulated`, prevents `live-verified` status, and fails `--require-live`.

## Optional CI and contributor verification

The `Optional live provider certification` workflow is manually runnable and
checks weekly only when both repository variables are configured:

- `OLYX_CERTIFICATION_CONFIG`: configuration JSON with one candidate per alias;
  credential values must be environment references, never inline values.
- `OLYX_CERTIFICATION_ALIASES`: a JSON string array, for example `["huggingface"]`.

Add the required credential as an Actions secret with the same name used by the
configuration. The workflow maps OPENAI_API_KEY and HF_TOKEN. Other compatible endpoints can
run the script locally or extend this explicit credential mapping.
Only the sanitized report is uploaded, retained for 30 days. Missing configured
credentials fail this optional workflow rather than silently claiming coverage.
No live credentials are used on pull requests.

A contributor or design partner can run the same command on their own account
and attach only its report to an issue/PR, stating the provider and tested public
model if they can share it. They do not need to send you credentials. Treat such
reports as contributor evidence: check provenance, version, date, and scope before
claiming support. An unsigned JSON file is not independently attested
provider certification.
