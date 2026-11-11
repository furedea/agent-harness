// Run against an installed Pi: node tests/pi_tools_conformance.mjs /absolute/path/to/pi/dist/index.js
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const moduleUrl = pathToFileURL(process.argv[2]).href;
registerHooks({ resolve(name, context, next) {
  return name === '@earendil-works/pi-coding-agent' ? { shortCircuit: true, url: moduleUrl } : next(name, context);
} });
const pi = await import(new URL('./core/tools/index.js', moduleUrl));
const root = mkdtempSync(join(tmpdir(), 'pi-conformance-'));
const cwd = process.cwd();
const savedPath = process.env.PATH;
const listeners = new Map();
try {
  mkdirSync(join(root, 'sandbox'));
  mkdirSync(join(root, 'bin'));
  for (const [source, destination] of [['bash_sandbox.ts', 'bash_sandbox.ts'], ['file_operations.mjs', 'sandbox/file_operations.mjs']]) {
    copyFileSync(new URL('../src/generation/assets/pi/' + source, import.meta.url), join(root, destination));
  }
  writeFileSync(join(root, 'sandbox_policy.json'), JSON.stringify({
    network: { allowedDomains: [], deniedDomains: [], allowUnixSockets: [] },
    filesystem: { denyRead: [], allowWrite: [], denyWrite: [] },
  }));
  writeFileSync(join(root, 'bin/srt'), `#!${process.execPath}
require('node:child_process').execFileSync('/bin/sh', ['-c', process.argv[5]], { stdio: 'inherit' });
`, { mode: 0o755 });
  process.env.PATH = join(root, 'bin') + ':' + savedPath;
  process.chdir(root);
  const tools = new Map(Object.entries(pi.createAllTools(root)));
  const { default: register } = await import(pathToFileURL(join(root, 'bash_sandbox.ts')));
  let active;
  register({ registerTool(tool) { tools.set(tool.name, tool); },
    getAllTools() { return [...tools.values()]; },
    setActiveTools(names) { active = names; },
    on(name, handler) { listeners.set(name, handler); },
  });
  tools.set('mcp_file', { name: 'mcp_file' });
  await listeners.get('before_agent_start')();
  const managed = ['bash', 'read', 'write', 'edit', 'grep', 'find', 'ls'];
  assert.deepEqual(active.sort(), managed.sort());
  // Enumerate the actual registry: every unmanaged tool must be inaccessible.
  for (const name of tools.keys()) {
    const decision = await listeners.get('tool_call')({ toolName: name });
    assert.equal(decision?.block === true, !managed.includes(name), name);
  }
  const file = join(root, 'example.txt');
  const call = (name, params) => tools.get(name).execute('id', params, undefined, undefined, { cwd: root });
  await call('write', { path: file, content: 'before\n' });
  assert.match((await call('read', { path: file })).content[0].text, /before/);
  await call('edit', { path: file, edits: [{ oldText: 'before', newText: 'after' }] });
  assert.equal(readFileSync(file, 'utf8'), 'after\n');
  assert.match((await call('ls', { path: root })).content[0].text, /example.txt/);
  for (const [name, params] of [['grep', { pattern: 'after', path: file }], ['find', { pattern: '*.txt', path: root }]]) {
    assert.ok((await call(name, params)).content.length, name);
  }
  rmSync(join(root, 'sandbox/file_operations.mjs'));
  for (const [name, params] of [['read', { path: file }], ['write', { path: file, content: 'escape' }],
    ['edit', { path: file, edits: [{ oldText: 'after', newText: 'escape' }] }],
    ['ls', { path: root }], ['grep', { pattern: 'after', path: file }], ['find', { pattern: '*.txt', path: root }]]) {
    await assert.rejects(() => call(name, params), undefined, name);
  }
  assert.equal(readFileSync(file, 'utf8'), 'after\n');
  console.log('Actual Pi registry and sandbox helper conformance passed');
} finally {
  await listeners.get('session_shutdown')?.();
  process.chdir(cwd);
  process.env.PATH = savedPath;
  rmSync(root, { recursive: true, force: true });
}
