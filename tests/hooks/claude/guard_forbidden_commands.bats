#!/usr/bin/env bats
# Tests for .claude/hooks/guard_forbidden_commands.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/guard_forbidden_commands.sh"
  RULES="$BATS_TEST_TMPDIR/forbidden_commands.json"
  cat >"$RULES" <<'JSON'
[
  {
    "pattern": ["rm"],
    "justification": "Do not delete files from Codex. Ask the user to run destructive cleanup manually."
  },
  {
    "pattern": ["git", "rm"],
    "justification": "Do not remove tracked files through shell commands from Codex."
  },
  {
    "pattern": ["bash", "-c"],
    "justification": "Do not hide shell commands inside bash -c from Codex policy checks."
  }
]
JSON
}

run_hook() {
  AGENT_FORBIDDEN_COMMAND_RULES="$RULES" bash "$HOOK" <<<"$(make_input "$1")"
}

@test "allows non-forbidden command" {
  run run_hook "git status"
  [ "$status" -eq 0 ]
}

@test "blocks rm prefix" {
  run run_hook "rm codex/hooks.json"
  [ "$status" -eq 2 ]
  [[ "$output" == *"forbidden command prefix"* ]]
  [[ "$output" == *"Do not delete files"* ]]
}

@test "blocks git rm prefix" {
  run run_hook "git rm codex/hooks.json"
  [ "$status" -eq 2 ]
  [[ "$output" == *"Do not remove tracked files"* ]]
}

@test "blocks forbidden segment after compound operator" {
  run run_hook "git status && rm codex/hooks.json"
  [ "$status" -eq 2 ]
  [[ "$output" == *"rm codex/hooks.json"* ]]
}

@test "blocks shell wrapper prefix from generated rules" {
  run run_hook "bash -c 'echo hello'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"Do not hide shell commands"* ]]
}

@test "blocks when generated rules file is missing" {
  AGENT_FORBIDDEN_COMMAND_RULES="$BATS_TEST_TMPDIR/missing.json" run bash "$HOOK" <<<"$(make_input "git status")"
  [ "$status" -eq 2 ]
  [[ "$output" == *"forbidden command rules were not found"* ]]
}
