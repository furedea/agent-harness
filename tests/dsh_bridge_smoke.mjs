import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = mkdtempSync(join(tmpdir(), 'dsh-bridge-'));
try {
  const adapter = join(root, 'adapter.py');
  writeFileSync(adapter, `import json, sys
payload = json.load(sys.stdin)
assert payload['tool_name'] == 'bash'
assert payload['tool_input'] == {'command': 'echo test'}
if sys.argv[-1] == 'tools/post-execute':
    assert payload['tool_response'] == {'value': 'done'}
    print(json.dumps({'systemMessage': 'lint feedback'}))
else:
    print(json.dumps({'decision': 'block', 'reason': 'forbidden'}))
`);
  process.env.AGENT_HARNESS_DSH_ADAPTER = adapter;
  const { apply } = await import('../src/generation/assets/dsh/hook_bridge.ts');
  const listeners = new Map();
  apply({on(event, listener) { listeners.set(event, listener); }});
  const sessionListeners = new Map();
  const agent = {};
  const signal = new AbortController().signal;
  apply({on(event, listener) { sessionListeners.set(event, listener); },
    workingDirectory: {async ensure(actualAgent, actualSignal) {
      assert.equal(actualAgent, agent);
      assert.equal(actualSignal, signal);
      return root;
    }}});
  writeFileSync(adapter, `import json, sys
p=json.load(sys.stdin)
assert p['cwd'] == ${JSON.stringify(root)}
print(json.dumps({'decision':'block','reason':'session cwd'}))
`);
  assert.equal((await sessionListeners.get('tools/pre-execute')({name: 'bash', arguments: {}, agent, signal}, async () => ({}))).reason, 'session cwd');
  writeFileSync(adapter, 'import json, sys\njson.load(sys.stdin)\nprint(json.dumps({"decision":"block","reason":"forbidden"}))\n');
  const exec = {name: 'bash', arguments: {command: 'echo test'}};
  const pre = listeners.get('tools/pre-execute');
  const denial = await pre(exec, () => { throw Error('must not continue'); });
  assert.equal(denial.kind, 'deny');
  assert.equal(denial.reason, 'forbidden');
  writeFileSync(adapter, 'import json, sys\njson.load(sys.stdin)\nprint("{}")\n');
  assert.deepEqual(await pre(exec, async () => ({kind: 'allow'})), {kind: 'allow'});
  writeFileSync(adapter, 'import json, sys\np=json.load(sys.stdin)\nassert p["tool_response"] == {"value":"done"}\nprint(json.dumps({"systemMessage":"lint feedback"}))\n');
  const post = await listeners.get('tools/post-execute')(exec, {value: 'done'}, async () => ({kind: 'accept'}));
  assert.equal(post.additionalContexts[0].content[0].text, 'lint feedback');
  assert.equal(typeof post.additionalContexts[0].id, 'string');
  assert.ok(post.additionalContexts[0].id.length > 0);
  assert.equal(post.additionalContexts[0].source.kind, 'agent-harness');
  const second = await listeners.get('tools/post-execute')(exec, {value: 'done'}, async () => ({}));
  assert.notEqual(post.additionalContexts[0].id, second.additionalContexts[0].id);
  writeFileSync(adapter, 'import sys\nsys.stderr.write("exit block")\nsys.exit(2)\n');
  assert.equal((await pre(exec, async () => ({kind: 'allow'}))).kind, 'deny');
  writeFileSync(adapter, 'import sys\nsys.stderr.write("adapter broken")\nsys.exit(1)\n');
  assert.equal((await pre(exec, async () => ({kind: 'allow'}))).kind, 'allow');
  rmSync(adapter);
  assert.equal((await pre(exec, async () => ({kind: 'allow'}))).kind, 'allow');
  console.log('dsh bridge block, continue, exit-2 and post context contracts passed');
} finally { rmSync(root, {recursive: true, force: true}); }
