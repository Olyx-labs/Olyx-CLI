# Olyx CLI

Route, replay, and debug model requests from your terminal.

```sh
npx olyx-cli --help
npx olyx-cli init --provider openai
npx olyx-cli route --input request.json
```

Requires Node.js 22 or newer. npm installs the matching prebuilt executable;
Rust, a C compiler, and a database server are not needed. SQLite is bundled.
Supported systems: macOS Apple Silicon and Linux x64/arm64.

For the shorter `oy` command:

```sh
npm install -g olyx-cli
oy --help
```

Set provider credentials in your environment. Olyx saves requests and responses
locally; keep the trace database private. External provider plugins may need
their own runtime.

[Documentation](https://docs.olyxai.io/getting-started/quickstart/) ·
[Source](https://github.com/Olyx-labs/Olyx-CLI)
