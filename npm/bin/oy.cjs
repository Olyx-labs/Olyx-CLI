#!/usr/bin/env node
'use strict';
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const platforms = new Set(['darwin-arm64', 'linux-arm64', 'linux-x64']);
function binaryPath(platform = process.platform, arch = process.arch, resolve = require.resolve) {
  const target = `${platform}-${arch}`;
  if (!platforms.has(target)) throw new Error(`Unsupported platform: ${target}. Supported: macOS Apple Silicon, Linux x64/arm64.`);
  const name = `olyx-cli-${target}`;
  let manifest;
  try { manifest = resolve(`${name}/package.json`); }
  catch { throw new Error(`Missing ${name}. Reinstall olyx-cli with optional dependencies enabled (omit --omit=optional).`); }
  const metadata = JSON.parse(fs.readFileSync(manifest, 'utf8'));
  const version = require('../package.json').version;
  if (metadata.version !== version) throw new Error(`Binary version ${metadata.version} does not match launcher ${version}. Reinstall olyx-cli.`);
  return path.join(path.dirname(manifest), 'bin', 'oy');
}
function main() {
  let binary;
  try { binary = binaryPath(); }
  catch (error) { console.error(`oy: ${error.message}`); process.exitCode = 1; return; }
  const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit', shell: false });
  const signals = ['SIGINT', 'SIGTERM'];
  const handlers = new Map(signals.map(signal => [signal, () => child.kill(signal)]));
  for (const [signal, handler] of handlers) process.on(signal, handler);
  const cleanup = () => { for (const [signal, handler] of handlers) process.removeListener(signal, handler); };
  child.once('error', error => { cleanup(); console.error(`oy: Could not start the installed binary (${error.code || 'unknown error'}). Reinstall olyx-cli.`); process.exitCode = 1; });
  child.once('exit', (code, signal) => {
    cleanup();
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code ?? 1;
  });
}
if (require.main === module) main();
module.exports = { binaryPath };
