#!/usr/bin/env bats
# Tests for .claude/hooks/guard_forbidden_commands.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/guard_forbidden_commands.sh"
  POLICY="$BATS_TEST_TMPDIR/command_permissions.json"
  RULES="$BATS_TEST_TMPDIR/forbidden_commands.json"
  cat >"$POLICY" <<'JSON'
{
  "version": 1,
  "rules": [
    {
      "decision": "deny",
      "prefix": ["rm"],
      "justification": "Do not delete files from Codex. Ask the user to run destructive cleanup manually."
    },
    {
      "decision": "deny",
      "prefix": ["git", "rm"],
      "justification": "Do not remove tracked files through shell commands from Codex."
    },
    {
      "decision": "deny",
      "prefix": ["bash", "-c"],
      "justification": "Do not hide shell commands inside bash -c from Codex policy checks."
    }
  ]
}
JSON
  cat >"$RULES" <<'JSON'
{
  "version": 1,
  "rules": [
    {
      "patterns": ["^never-match-this-command$"],
      "justification": "Test-only fine-grained forbidden rule."
    }
  ]
}
JSON
}

run_hook() {
  AGENT_COMMAND_PERMISSIONS="$POLICY" AGENT_FORBIDDEN_COMMAND_RULES="$RULES" \
    bash "$HOOK" <<<"$(make_input "$1")"
}

run_hook_with_global_rules() {
  AGENT_COMMAND_PERMISSIONS="$REPO_ROOT/agents/command_permissions.json" \
    AGENT_FORBIDDEN_COMMAND_RULES="$REPO_ROOT/agents/hooks/rules/forbidden_commands.json" \
    bash "$HOOK" <<<"$(make_input "$1")"
}

@test "allows non-forbidden command" {
  run run_hook "git status"
  [ "$status" -eq 0 ]
}

@test "project rules forbid a precise command" {
  create_temp_git_repo
  mkdir -p "$TEMP_REPO/.agents/hooks/rules"
  cat >"$TEMP_REPO/.agents/hooks/rules/forbidden_commands.json" <<'JSON'
{
  "version": 1,
  "rules": [
    {
      "patterns": ["^git status --porcelain$"],
      "justification": "Use the repository status wrapper instead."
    }
  ]
}
JSON

  CLAUDE_PROJECT_DIR="$TEMP_REPO" run run_hook "git status --porcelain"

  [ "$status" -eq 2 ]
  [[ "$output" == *"Use the repository status wrapper instead."* ]]
}

@test "blocks invalid project forbidden command regex" {
  create_temp_git_repo
  mkdir -p "$TEMP_REPO/.agents/hooks/rules"
  cat >"$TEMP_REPO/.agents/hooks/rules/forbidden_commands.json" <<'JSON'
{
  "version": 1,
  "rules": [
    {
      "patterns": ["["],
      "justification": "Invalid test regex."
    }
  ]
}
JSON

  CLAUDE_PROJECT_DIR="$TEMP_REPO" run run_hook "git status"

  [ "$status" -eq 2 ]
  [[ "$output" == *"invalid project forbidden command rules"* ]]
}

@test "global regex rules block policy bypass forms" {
  local _command

  run run_hook_with_global_rules ".venv/bin/python -c 'print(1)'"
  [ "$status" -eq 2 ]

  run run_hook_with_global_rules "./.venv/bin/python3 scripts/run_audit.py"
  [ "$status" -eq 2 ]

  run run_hook_with_global_rules "/tmp/project/.venv/bin/python -m pytest"
  [ "$status" -eq 2 ]

  run run_hook_with_global_rules "git add ."
  [ "$status" -eq 2 ]

  for _command in \
    "git add -A" \
    "git add --all" \
    "ls && git add ." \
    "echo ok | xargs -I {} git add -A" \
    "git   add   ." \
    "git add --all --verbose"; do
    run run_hook_with_global_rules "$_command"
    [ "$status" -eq 2 ]
  done

  run run_hook_with_global_rules "git commit --no-verify -m example"
  [ "$status" -eq 2 ]

  for _command in \
    "git commit -m test --no-verify" \
    "git commit -n -m test" \
    "git   commit   --no-verify"; do
    run run_hook_with_global_rules "$_command"
    [ "$status" -eq 2 ]
  done
}

@test "blocks rm prefix" {
  run run_hook "rm codex/hooks.json"
  [ "$status" -eq 2 ]
  [[ "$output" == *"forbidden command"* ]]
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
  AGENT_COMMAND_PERMISSIONS="$POLICY" AGENT_FORBIDDEN_COMMAND_RULES="$BATS_TEST_TMPDIR/missing.json" \
    run bash "$HOOK" <<<"$(make_input "git status")"
  [ "$status" -eq 2 ]
  [[ "$output" == *"forbidden command regex rules were not found or are invalid"* ]]
}
