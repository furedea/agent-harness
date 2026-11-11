import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = mkdtempSync(join(tmpdir(), 'pi-bridge-'));
try {
  const adapter = join(root, 'adapter.py');
  writeFileSync(adapter, '');
  process.env.AGENT_HARNESS_PI_ADAPTER = adapter;
  const { default: register } = await import('../src/generation/assets/pi/hook_bridge.ts');
  const listeners = new Map();
  register({ on(event, listener) { listeners.set(event, listener); } });
  const event = { toolName: 'bash', input: { command: 'git push origin main' } };
  let prompts = [];
  let answer = 'Yes';
  const ctx = {
    cwd: root, hasUI: true, sessionManager: { getSessionId: () => 'test' },
    ui: { async select(title, options) { prompts.push(title); assert.deepEqual(options, ['Yes', 'No']); return answer; } },
  };
  const ask = { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'ask', permissionDecisionReason: 'Confirm push' } };
  function output(values, code = 0, stderr = '') {
    prompts = [];
    writeFileSync(adapter, `import sys
sys.stdin.read()
sys.stdout.write(${JSON.stringify(values.map(value => typeof value === 'string' ? value : JSON.stringify(value)).join('\n'))})
sys.stderr.write(${JSON.stringify(stderr)})
sys.exit(${code})
`);
  }
  const call = () => listeners.get('tool_call')(event, ctx);
  output([ask]);
  assert.equal(await call(), undefined);
  assert.equal(prompts.length, 1);
  assert.ok(prompts[0].includes('Confirm push'));
  assert.ok(prompts[0].includes('bash'));
  assert.ok(prompts[0].includes(event.input.command));
  for (answer of ['No', undefined, 'unexpected']) {
    output([ask]);
    assert.deepEqual(await call(), { block: true, reason: 'Blocked by user: Confirm push' });
  }
  ctx.hasUI = false;
  output([ask]);
  assert.deepEqual(await call(), { block: true, reason: 'Confirm push (no UI to confirm; blocked)' });
  assert.equal(prompts.length, 0);
  ctx.hasUI = true;
  answer = 'Yes';
  const allow = { hookSpecificOutput: { permissionDecision: 'allow' } };
  for (const deny of [
    { decision: 'block', reason: 'Denied' },
    { action: 'block', message: 'Denied' },
    { hookSpecificOutput: { permissionDecision: 'deny', permissionDecisionReason: 'Denied' } },
  ]) {
    for (const values of [[deny, ask, allow], [allow, ask, deny]]) {
      output(values);
      assert.deepEqual(await call(), { block: true, reason: 'Denied' });
      assert.equal(prompts.length, 0);
    }
  }
  for (const values of [[ask, allow], [allow, ask], ['noise', '{invalid', ask]]) {
    output(values);
    assert.equal(await call(), undefined);
    assert.equal(prompts.length, 1);
  }
  output([ask], 2, 'Exit denied');
  assert.deepEqual(await call(), { block: true, reason: 'Exit denied' });
  assert.equal(prompts.length, 0);
  output([{ decision: 'block' }]);
  assert.equal((await call()).block, true);
  output([allow]);
  assert.equal(await call(), undefined);
  assert.equal(prompts.length, 0);
  output([ask]);
  await listeners.get('tool_call')({ toolName: 'read', input: { path: 'README.md' } }, ctx);
  assert.ok(prompts[0].includes('read'));
  assert.ok(prompts[0].includes('README.md'));
} finally {
  rmSync(root, { recursive: true, force: true });
}
