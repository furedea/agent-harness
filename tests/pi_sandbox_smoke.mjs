import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

// Model only Pi's documented spawnHook boundary; the fake srt records actual argv.
registerHooks({ resolve(specifier, context, next) {
  if (specifier !== '@earendil-works/pi-coding-agent') return next(specifier, context);
  return { shortCircuit: true, url: 'data:text/javascript,' + encodeURIComponent(`
    import { execFileSync } from 'node:child_process';
    export function createBashTool(cwd, { spawnHook }) {
      return { name: 'bash', async execute(id, params) {
        const run = spawnHook({ command: params.command, cwd, env: process.env });
        return execFileSync('/bin/sh', ['-c', run.command], { cwd: run.cwd, env: run.env }).toString();
      }};
    }
  `) };
}});
const originalCwd = process.cwd();
const root = mkdtempSync(join(tmpdir(), 'pi-sandbox-'));
const cleanups = [];
try {
  const repo = join(root, 'repo');
  const worktree = join(root, "work tree's");
  execFileSync('git', ['init', repo]);
  execFileSync('git', ['-C', repo, '-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '--allow-empty', '-m', 'initial']);
  execFileSync('git', ['-C', repo, 'worktree', 'add', '-b', 'test', worktree]);
  const extension = join(root, 'bash_sandbox.ts');
  copyFileSync(new URL('../src/generation/assets/pi/bash_sandbox.ts', import.meta.url), extension);
  const policyPath = join(root, 'sandbox_policy.json');
  const policy = {
    network: { allowedDomains: ['github.com'], deniedDomains: [], allowUnixSockets: ['/nix/var/nix/daemon-socket/socket'] },
    filesystem: { denyRead: ['~/.pi/agent/auth.json'], allowWrite: [], denyWrite: [] },
  };
  writeFileSync(policyPath, JSON.stringify(policy));
  const bin = join(root, 'bin');
  mkdirSync(bin);
  // Absolute interpreter: build sandboxes may have no /usr/bin/env for the shebang to resolve.
  writeFileSync(join(bin, 'srt'), `#!${process.execPath}
const fs = require('node:fs');
const args = process.argv.slice(2);
const settings = JSON.parse(fs.readFileSync(args[1], 'utf8'));
fs.writeFileSync(${JSON.stringify(join(root, 'record.json'))}, JSON.stringify({ args, settings }));
`, { mode: 0o755 });
  process.env.PATH = bin + ':' + process.env.PATH;
  process.chdir(worktree);
  const { default: register } = await import(pathToFileURL(extension));
  let tool;
  const listeners = new Map();
  register({ registerTool(value) { tool = value; }, on(event, fn) { listeners.set(event, fn); if (event === 'session_shutdown') cleanups.push(fn); } });
  assert.equal(tool.name, 'bash');
  const command = "printf '%s' \"a'b; $(echo injection)\"\nfalse";
  await tool.execute('id', { command });
  const record = JSON.parse(readFileSync(join(root, 'record.json')));
  assert.deepEqual(record.args.slice(2), ['-c', command]);
  assert.equal(record.args[0], '--settings');
  assert.deepEqual(record.settings.network.allowedDomains, ['github.com']);
  for (const path of [realpathSync(worktree), realpathSync(join(repo, '.git'))]) {
    assert.ok(record.settings.filesystem.allowWrite.includes(path));
  }
  for (const path of [join(realpathSync(repo), '.git/hooks'), join(realpathSync(repo), '.git/config'), join(realpathSync(repo), '.git/worktrees/*/config.worktree')]) {
    assert.ok(record.settings.filesystem.denyWrite.includes(path));
  }
  // A linked worktree's .git gitfile must not be redirectable from inside the sandbox.
  assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(worktree), '.git')));
  // Pi supplies a per-call cwd; the policy must follow it, and non-git dirs must still run.
  const plain = join(root, 'plain');
  mkdirSync(plain);
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: plain });
  const plainRecord = JSON.parse(readFileSync(join(root, 'record.json')));
  assert.ok(plainRecord.settings.filesystem.allowWrite.includes(realpathSync(plain)));
  assert.ok(!plainRecord.settings.filesystem.allowWrite.includes(realpathSync(worktree)));
  assert.ok(plainRecord.settings.filesystem.denyWrite.every(path => !path.endsWith('config.worktree')));
  // A .git planted later would make git run attacker config outside the sandbox, so it is denied even when absent.
  assert.ok(plainRecord.settings.filesystem.denyWrite.includes(join(realpathSync(plain), '.git')));
  // The policy is rebuilt per call: a directory that becomes a repository gets the repository rules.
  execFileSync('git', ['init', plain]);
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: plain });
  const initRecord = JSON.parse(readFileSync(join(root, 'record.json')));
  assert.ok(initRecord.settings.filesystem.denyWrite.some(path => path.endsWith('.git/hooks')));
  assert.ok(!initRecord.settings.filesystem.denyWrite.includes(join(realpathSync(plain), '.git')));
  // Startup failures must still replace bash, and execution must refuse.
  for (const invalid of ['{}', '{', JSON.stringify({ ...policy, network: { ...policy.network, deniedDomains: undefined } }),
    ...['https://bad', '*.com', 'host', 'example.com:01', '*'].map(value => JSON.stringify({ ...policy, network: { ...policy.network, allowedDomains: [value] } }))]) {
    writeFileSync(policyPath, invalid);
    register({ registerTool(value) { tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
    await assert.rejects(() => tool.execute('id', { command: 'touch escaped' }));
  }
  writeFileSync(policyPath, JSON.stringify(policy));
  register({ registerTool(value) { tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
  rmSync(join(bin, 'srt'));
  await assert.rejects(() => tool.execute('id', { command: 'touch escaped' }));
} finally {
  for (const cleanup of cleanups) await cleanup();
  process.chdir(originalCwd);
  rmSync(root, { recursive: true, force: true });
}
