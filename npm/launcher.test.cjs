'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { binaryPath } = require('./bin/oy.cjs');

test('unsupported platforms fail before package resolution', () => {
  assert.throws(() => binaryPath('darwin', 'x64', () => assert.fail('must not resolve')), /Unsupported platform/);
});
test('missing optional packages have an actionable error', () => {
  assert.throws(() => binaryPath('linux', 'x64', () => { throw new Error('not installed'); }), /optional dependencies enabled/);
});
test('platform selection, executable names and versions stay aligned', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'olyx-launcher-'));
  try {
    const file = path.join(directory, 'package.json');
    const version = require('./package.json').version;
    fs.writeFileSync(file, JSON.stringify({ version }));
    for (const [system, cpu] of [['darwin', 'arm64'], ['linux', 'arm64'], ['linux', 'x64']]) {
      assert.equal(binaryPath(system, cpu, name => {
        assert.equal(name, `olyx-cli-${system}-${cpu}/package.json`);
        return file;
      }), path.join(directory, 'bin', 'oy'));
    }
    fs.writeFileSync(file, JSON.stringify({ version: '0.0.0' }));
    assert.throws(() => binaryPath('linux', 'x64', () => file), /does not match launcher/);
  } finally { fs.rmSync(directory, { recursive: true }); }
});
