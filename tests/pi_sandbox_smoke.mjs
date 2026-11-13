import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, copyFileSync, symlinkSync, writeFileSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { pathToFileURL } from 'node:url';

// Model only Pi's documented spawnHook boundary; the fake srt records actual argv.
registerHooks({ resolve(specifier, context, next) {
  if (specifier !== '@earendil-works/pi-coding-agent') return next(specifier, context);
  return { shortCircuit: true, url: 'data:text/javascript,' + encodeURIComponent(`
    import { execFileSync } from 'node:child_process';
    import { readFile, writeFile, mkdir, access } from 'node:fs/promises';
    export const createReadTool = (cwd, { operations } = {}) => ({ name: 'read', async execute(id, p) {
      if (!operations) throw new Error('unsandboxed read');
      await operations.access(p.path);
      await operations.detectImageMimeType(p.path);
      return (await operations.readFile(p.path)).toString();
    }});
    export const createWriteTool = (cwd, { operations } = {}) => ({ name: 'write', async execute(id, p) {
      if (!operations) throw new Error('unsandboxed write');
      await operations.mkdir(cwd);
      await operations.writeFile(p.path, p.content);
    }});
    export const createEditTool = (cwd, { operations } = {}) => ({ name: 'edit', async execute(id, p) {
      if (!operations) throw new Error('unsandboxed edit');
      await operations.access(p.path);
      const before = (await operations.readFile(p.path)).toString();
      await operations.writeFile(p.path, before.replace(p.oldText, p.newText));
    }});
    const search = name => (cwd) => ({ name, async execute(id, p) {
      // Runs in the helper; its output proves the child, rather than parent, opened the file.
      return { content: [{ type: 'text', text: (await readFile(p.path)).toString() }] };
    }});
    export const createGrepTool = search('grep');
    export const createFindTool = search('find');
    export const createLsTool = search('ls');
    export function createBashTool(cwd, { spawnHook }) {
      return { name: 'bash', async execute(id, params) {
        const run = spawnHook({ command: params.command, cwd, env: process.env });
        return execFileSync('/bin/sh', ['-c', run.command], { cwd: run.cwd, env: run.env }).toString();
      }};
    }
  `) };
}});
const originalCwd = process.cwd();
// Repository discovery must see only the repositories this test creates: git variables set by a
// calling hook, or a temporary directory inside another repository, would make the "a directory
// becomes a repository" assertions below find the outer one both before and after.
for (const name of Object.keys(process.env)) {
  if (name.startsWith('GIT_') && !name.startsWith('GIT_CONFIG')) delete process.env[name];
}
const root = mkdtempSync(join(tmpdir(), 'pi-sandbox-'));
process.env.GIT_CEILING_DIRECTORIES = realpathSync(tmpdir());
const cleanups = [];
try {
  const repo = join(root, 'repo');
  writeFileSync(join(root, 'target'), 'x');
  symlinkSync(join(root, 'target'), join(root, 'link-to-target'));
  mkdirSync(join(root, 'real-dir'));
  symlinkSync(join(root, 'real-dir'), join(root, 'linked-dir'));
  const worktree = join(root, "work tree's");
  execFileSync('git', ['init', repo]);
  execFileSync('git', ['-C', repo, '-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '--allow-empty', '-m', 'initial']);
  execFileSync('git', ['-C', repo, 'worktree', 'add', '-b', 'test', worktree]);
  const extension = join(root, 'bash_sandbox.ts');
  copyFileSync(new URL('../src/generation/assets/pi/bash_sandbox.ts', import.meta.url), extension);
  mkdirSync(join(root, 'sandbox'));
  const helper = join(root, 'sandbox/file_operations.mjs');
  const helperSource = new URL('../src/generation/assets/pi/file_operations.mjs', import.meta.url);
  const policyPath = join(root, 'sandbox_policy.json');
  const policy = {
    network: { allowedDomains: ['github.com'], deniedDomains: [], allowUnixSockets: ['/nix/var/nix/daemon-socket/socket'] },
    filesystem: { denyRead: ['~/.pi/agent/auth.json', '**/.env'], allowWrite: [], denyWrite: [join(root, 'link-to-target'), join(root, 'linked-dir/inner')] },
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
if (args[3].includes('file_operations.mjs')) {
  const input = fs.readFileSync(0, 'utf8');
  fs.appendFileSync(${JSON.stringify(join(root, 'operations.jsonl'))}, JSON.stringify(JSON.parse(input)) + '\\n');
  require('node:child_process').execFileSync('/bin/sh', ['-c', args[3]], { input, stdio: ['pipe', 'inherit', 'inherit'] });
}
`, { mode: 0o755 });
  process.env.PATH = bin + ':' + process.env.PATH;
  process.chdir(worktree);
  const { default: register } = await import(pathToFileURL(extension));
  let tool;
  const tools = new Map();
  const listeners = new Map();
  register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { listeners.set(event, fn); if (event === 'session_shutdown') cleanups.push(fn); } });
  assert.deepEqual([...tools.keys()].sort(), ['bash', 'edit', 'find', 'grep', 'ls', 'read', 'write']);
  const guardedCall = listeners.get('tool_call');
  assert.equal(await guardedCall({ toolName: 'read' }), undefined);
  tools.set('mcp_files', { name: 'mcp_files' });
  assert.equal((await guardedCall({ toolName: 'mcp_files' })).block, true);
  assert.equal(await guardedCall({ toolName: 'read' }), undefined);
  tools.delete('mcp_files');
  copyFileSync(helperSource, helper);
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
  for (const suffix of ['config.worktree', 'modules/**/config', 'modules/**/config.worktree', 'modules/**/hooks', 'modules/**/commondir', 'modules/**/gitdir', 'worktrees/*/modules/**/config', 'worktrees/*/modules/**/hooks', 'worktrees/*/modules/**/commondir', 'worktrees/*/modules/**/gitdir', 'worktrees/*/hooks', 'worktrees/*/config']) {
    assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(repo), '.git', suffix)), suffix);
  }
  // Nested repositories and submodule gitfiles inside the worktree are denied; the worktree's own .git is not.
  assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(worktree), '*/**/.git')));
  assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(worktree), '.pi')));
  // A relative secret glob is anchored at the repository root (here the worktree), where srt would otherwise resolve it against the cwd.
  assert.ok(record.settings.filesystem.denyRead.includes(join(realpathSync(worktree), '**/.env')));
  const file = join(worktree, "data ' $(false).txt");
  await tools.get('write').execute('w', { path: file, content: 'before' });
  assert.equal(readFileSync(file, 'utf8'), 'before');
  assert.equal(await tools.get('read').execute('r', { path: file }), 'before');
  await tools.get('edit').execute('e', { path: file, oldText: 'before', newText: 'after' });
  assert.equal(readFileSync(file, 'utf8'), 'after');
  for (const name of ['grep', 'find', 'ls']) {
    assert.equal((await tools.get(name).execute('s', { path: file })).content[0].text, 'after');
  }
  const operations = readFileSync(join(root, 'operations.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line).operation);
  assert.deepEqual(operations, ['mkdir', 'writeFile', 'access', 'detectImageMimeType', 'readFile',
    'access', 'readFile', 'writeFile', 'search', 'search', 'search']);
  // Cancellation reports which filesystem operation and path were interrupted.
  const cancelled = AbortSignal.abort();
  await assert.rejects(() => tools.get('read').execute('cancelled', { path: file }, cancelled), error => {
    assert.ok(error.message.includes('access'), error.message);
    assert.ok(error.message.includes(file), error.message);
    return true;
  });
  // Removing the helper must prevent all file operations rather than fall back to local I/O.
  rmSync(helper);
  for (const [name, params] of [['read', { path: file }], ['write', { path: file, content: 'escaped' }],
    ['edit', { path: file, oldText: 'after', newText: 'escaped' }],
    ...['grep', 'find', 'ls'].map(name => [name, { path: file }])]) {
    await assert.rejects(() => tools.get(name).execute('missing', params), undefined, name);
  }
  assert.equal(readFileSync(file, 'utf8'), 'after');
  copyFileSync(helperSource, helper);
  // Bracketed IPv6 literals are valid srt domains and must not disable bash.
  writeFileSync(policyPath, JSON.stringify({ ...policy, network: { ...policy.network, allowedDomains: ['github.com', '[2001:db8::1]:443'] } }));
  register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
  await tool.execute('id', { command: 'true' });
  writeFileSync(policyPath, JSON.stringify(policy));
  register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
  // Files git reads to locate its config must not be repointable either.
  for (const suffix of ['commondir', 'worktrees/*/commondir', 'worktrees/*/gitdir']) {
    assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(repo), '.git', suffix)), suffix);
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
  // From a subdirectory, a relative secret glob stays anchored at the repository root, so the parent's .env is denied.
  const subdirectory = join(worktree, 'nested/deeper');
  mkdirSync(subdirectory, { recursive: true });
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: subdirectory });
  const subdirectoryRecord = JSON.parse(readFileSync(join(root, 'record.json')));
  assert.ok(subdirectoryRecord.settings.filesystem.denyRead.includes(join(realpathSync(worktree), '**/.env')));
  // The cwd anchoring stays as well, so an entry written for it keeps its reach.
  assert.ok(subdirectoryRecord.settings.filesystem.denyRead.includes(join(realpathSync(subdirectory), '**/.env')));
  // Repository-selection variables inherited from a Git hook must not redirect discovery to another repository.
  const hooked = join(worktree, 'hooked');
  mkdirSync(hooked);
  Object.assign(process.env, { GIT_DIR: join(repo, '.git'), GIT_WORK_TREE: repo, GIT_COMMON_DIR: join(repo, '.git') });
  try {
    await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: hooked });
  } finally {
    for (const name of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR']) delete process.env[name];
  }
  const hookedDeny = JSON.parse(readFileSync(join(root, 'record.json'))).settings.filesystem.denyRead;
  assert.ok(hookedDeny.includes(join(realpathSync(worktree), '**/.env')));
  assert.ok(!hookedDeny.includes(join(realpathSync(repo), '**/.env')));
  // Outside a repository the working directory is the root.
  assert.ok(plainRecord.settings.filesystem.denyRead.includes(join(realpathSync(plain), '**/.env')));
  // A .git planted later would make git run attacker config outside the sandbox, so it is denied even when absent.
  assert.ok(plainRecord.settings.filesystem.denyWrite.includes(join(realpathSync(plain), '.git')));
  // A protected symlink keeps its own deny rule (the link can be replaced) besides its target's.
  for (const path of [join(realpathSync(root), 'link-to-target'), join(realpathSync(root), 'target')]) {
    assert.ok(record.settings.filesystem.denyWrite.includes(path), path);
  }
  assert.ok(record.settings.filesystem.denyWrite.includes(join(realpathSync(root), 'linked-dir')), 'symlinked ancestor');
  // The first resolution is final: a directory that becomes a repository mid-session is refused, not re-trusted.
  execFileSync('git', ['init', plain]);
  await assert.rejects(() => tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: plain }), /git layout changed/);
  // When repository discovery fails, .git must stay protected rather than falling back to the plain cwd rule.
  const broken = join(root, 'broken');
  mkdirSync(join(broken, '.git'), { recursive: true });
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: broken });
  assert.ok(JSON.parse(readFileSync(join(root, 'record.json'))).settings.filesystem.denyWrite.includes(join(realpathSync(broken), '.git')));
  // The common git dir is pinned per cwd: repointing .git from inside the sandbox must not widen allowWrite.
  const victim = join(root, 'victim');
  mkdirSync(victim);
  const pinned = join(root, 'pinned');
  execFileSync('git', ['init', pinned]);
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: pinned });
  rmSync(join(pinned, '.git'), { recursive: true });
  writeFileSync(join(pinned, '.git'), 'gitdir: ' + victim + '\n');
  await assert.rejects(() => tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: pinned }), /git layout changed/);
  // A common dir whose name ends in whitespace is kept verbatim, not trimmed into a different path.
  const spaced = join(root, 'spaced repo ');
  execFileSync('git', ['init', spaced]);
  await tool.execute('id', { command: 'true' }, undefined, undefined, { cwd: spaced });
  assert.ok(JSON.parse(readFileSync(join(root, 'record.json'))).settings.filesystem.allowWrite.includes(join(realpathSync(spaced), '.git')));
  // Startup failures must still replace bash, and execution must refuse.
  for (const invalid of ['{}', '{', JSON.stringify({ ...policy, network: { ...policy.network, deniedDomains: undefined } }),
    ...['https://bad', '*.com', 'host', 'example.com:01', '*'].map(value => JSON.stringify({ ...policy, network: { ...policy.network, allowedDomains: [value] } }))]) {
    writeFileSync(policyPath, invalid);
    register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
    await assert.rejects(() => tool.execute('id', { command: 'touch escaped' }));
    for (const name of ['read', 'write', 'edit', 'grep', 'find', 'ls']) {
      await assert.rejects(() => tools.get(name).execute('bad', { path: file, content: 'escape' }), undefined, name);
    }
  }
  // A rejected policy names the offending field, not only that validation failed.
  writeFileSync(policyPath, JSON.stringify({ ...policy, network: { ...policy.network, allowedDomains: ['https://bad'] } }));
  register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
  await assert.rejects(() => tool.execute('id', { command: 'true' }), /network\.allowedDomains has an invalid domain "https:\/\/bad"/);
  writeFileSync(policyPath, JSON.stringify(policy));
  register({ getAllTools() { return [...tools.values()]; }, registerTool(value) { tools.set(value.name, value); if (value.name === 'bash') tool = value; }, on(event, fn) { if (event === 'session_shutdown') cleanups.push(fn); } });
  rmSync(join(bin, 'srt'));
  // A host may install a real srt; hide it but keep git, which may share its directory.
  const hostDirs = process.env.PATH.split(delimiter);
  const gitDir = hostDirs.find(dir => existsSync(join(dir, 'git')));
  mkdirSync(join(root, 'git-only'));
  symlinkSync(join(gitDir, 'git'), join(root, 'git-only/git'));
  process.env.PATH = [join(root, 'git-only'), ...hostDirs.filter(dir => !existsSync(join(dir, 'srt')))].join(delimiter);
  await assert.rejects(() => tool.execute('id', { command: 'touch escaped' }), /srt: not found/);
  for (const name of ['read', 'write', 'edit', 'grep', 'find', 'ls']) {
    await assert.rejects(() => tools.get(name).execute('missing-srt', { path: file, content: 'escape' }), undefined, name);
  }
} finally {
  for (const cleanup of cleanups) await cleanup();
  process.chdir(originalCwd);
  rmSync(root, { recursive: true, force: true });
}
