#!/usr/bin/env bats
# Tests for codex/hooks/adapt_shell_command.sh

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  HOOK="$REPO_ROOT/codex/hooks/adapt_shell_command.sh"
}

@test "prints usage without hook path" {
  run "$HOOK"
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage:"* ]]
}

@test "passes tool_input command to shared hook as command" {
  stub_dir="$(mktemp -d "$BATS_TEST_TMPDIR/stub.XXXXXX")"
  stub_hook="$stub_dir/hook.sh"
  cat >"$stub_hook" <<'STUB'
#!/bin/bash
jq -r '.tool_input.command'
STUB
  chmod +x "$stub_hook"

  input='{"tool_input":{"command":"git status"}}'
  run bash -c "printf '%s' '$input' | '$HOOK' '$stub_hook'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"git status"* ]]
}

@test "successful pass-through does not emit shell trace" {
  stub_dir="$(mktemp -d "$BATS_TEST_TMPDIR/stub.XXXXXX")"
  stub_hook="$stub_dir/hook.sh"
  cat >"$stub_hook" <<'STUB'
#!/bin/bash
cat >/dev/null
STUB
  chmod +x "$stub_hook"

  input='{"tool_input":{"cmd":"pwd"}}'
  run bash -c "printf '%s' '$input' | '$HOOK' '$stub_hook'"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "passes exec_command cmd to shared hook as command" {
  stub_dir="$(mktemp -d "$BATS_TEST_TMPDIR/stub.XXXXXX")"
  stub_hook="$stub_dir/hook.sh"
  cat >"$stub_hook" <<'STUB'
#!/bin/bash
jq -r '.tool_input.command'
STUB
  chmod +x "$stub_hook"

  input='{"tool_input":{"cmd":"rm -rf /tmp/example"}}'
  run bash -c "printf '%s' '$input' | '$HOOK' '$stub_hook'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"rm -rf /tmp/example"* ]]
}

@test "runs shared hook from payload cwd" {
  stub_dir="$(mktemp -d "$BATS_TEST_TMPDIR/stub.XXXXXX")"
  work_dir="$(mktemp -d "$BATS_TEST_TMPDIR/work.XXXXXX")"
  stub_hook="$stub_dir/hook.sh"
  cat >"$stub_hook" <<'STUB'
#!/bin/bash
pwd
cat >/dev/null
STUB
  chmod +x "$stub_hook"

  input="$(jq -n --arg cwd "$work_dir" '{"cwd":$cwd,"tool_input":{"cmd":"git status"}}')"
  run bash -c "printf '%s' '$input' | '$HOOK' '$stub_hook'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"$work_dir"* ]]
}

@test "blocks non-executable shared hook" {
  stub_dir="$(mktemp -d "$BATS_TEST_TMPDIR/stub.XXXXXX")"
  stub_hook="$stub_dir/hook.sh"
  touch "$stub_hook"

  input='{"tool_input":{"cmd":"git status"}}'
  run bash -c "printf '%s' '$input' | '$HOOK' '$stub_hook'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}
