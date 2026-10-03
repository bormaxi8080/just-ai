const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Module = require('node:module');
let trusted = true;
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === 'vscode') { return { workspace: { get isTrusted() { return trusted; } } }; }
  return originalLoad.call(this, request, parent, isMain);
};
const { JustAiClient } = require('../out/client.js');
Module._load = originalLoad;

test('companion receives the configured just binary, valid argv and JSON contracts', { skip: process.platform === 'win32' }, async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'just-ai-client-'));
  try {
    const binary = path.join(root, process.platform === 'win32' ? 'companion.cmd' : 'companion');
    const script = path.join(root, 'companion.cjs');
    fs.writeFileSync(script, `const fs = require('node:fs'); const args = process.argv.slice(2); fs.writeFileSync('argv.json', JSON.stringify(args)); const command = args[2]; if (command === 'export-context') console.log(JSON.stringify({recipes: [], modules: [], warnings: []})); else if (command === 'doctor') console.log(JSON.stringify({recipes: [], total_recipes: 0})); else if (command === 'history') console.log('[]'); else console.log('ok');`);
    fs.writeFileSync(binary, `#!/bin/sh\nexec "${process.execPath}" "${script}" "$@"\n`, { mode: 0o755 });
    const client = new JustAiClient(root, '/configured/just', binary);
    assert.deepEqual((await client.getProjectContext()).recipes, []);
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'argv.json'))), ['--just-binary', '/configured/just', 'export-context']);
    assert.equal((await client.runDoctor(true)).total_recipes, 0);
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'argv.json'))), ['--just-binary', '/configured/just', 'doctor', '--json']);
    await client.add('recipe request', false);
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'argv.json'))), ['--just-binary', '/configured/just', 'add', 'recipe request']);
    await client.runRecipe('deploy; echo injected', 'run deploy; echo injected');
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'argv.json'))).slice(2), ['run', '--yes', '--confirm', 'run deploy; echo injected', '--', 'deploy; echo injected']);
    assert.deepEqual(await client.getHistory(10, 'test', false), []);
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'argv.json'))).slice(2), ['history', 'recent', '--limit', '10', '--recipe', 'test', '--success', 'false', '--json']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('missing executable rejects instead of producing an unhandled child error', async () => {
  const client = new JustAiClient(os.tmpdir(), 'just', path.join(os.tmpdir(), 'missing-just-ai-client-executable'));
  await assert.rejects(client.getProjectContext(), { code: 'ENOENT' });
});

test('untrusted workspaces cannot spawn companion processes', async () => {
  trusted = false;
  try { await assert.rejects(new JustAiClient(os.tmpdir()).getProjectContext(), /Trust this workspace/); }
  finally { trusted = true; }
});
