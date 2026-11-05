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
  writeFileSync(adapter, 'import sys\nsys.stderr.write("exit block")\nsys.exit(2)\n');
  assert.equal((await pre(exec, async () => ({kind: 'allow'}))).kind, 'deny');
  console.log('dsh bridge block, continue, exit-2 and post context contracts passed');
} finally { rmSync(root, {recursive: true, force: true}); }
