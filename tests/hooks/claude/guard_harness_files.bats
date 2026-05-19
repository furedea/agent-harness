#!/usr/bin/env bats
# Tests for .claude/hooks/guard_harness_files.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/guard_harness_files.sh"
  LOG_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/harness.XXXXXX")"
}

teardown() {
  [ -d "${LOG_TMPDIR:-}" ] && rm -rf "$LOG_TMPDIR"
}

make_edit_input() {
  jq -n --arg tool "${1:-Edit}" --arg file_path "$2" --arg session "sess-harness" \
    '{"tool_name":$tool,"tool_input":{"file_path":$file_path},"session_id":$session}'
}

get_last_log() {
  cat "$LOG_TMPDIR/docs/logs/audit/"*.jsonl 2>/dev/null | tail -1
}

@test "blocks edits to Claude hook symlink path" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Edit "$HOME/.claude/hooks/guard_allowed_commands.sh")"

  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
  [[ "$output" == *"agent harness boundary"* ]]
}

@test "blocks writes to harness hook source path" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Write "$REPO_ROOT/agents/hooks/guard_allowed_commands.sh")"

  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

@test "blocks MultiEdit to Codex hook source path" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input MultiEdit "$REPO_ROOT/codex/hooks/adapt_lint_format.sh")"

  [ "$status" -eq 2 ]
}

@test "blocks generated Claude settings" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Edit "$HOME/.claude/settings.json")"

  [ "$status" -eq 2 ]
}

@test "blocks generated forbidden command rules" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Edit "$HOME/.claude/rules/forbidden_commands.json")"

  [ "$status" -eq 2 ]
}

@test "allows normal project file edits" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Edit "$REPO_ROOT/src/app.py")"

  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "allows empty file path" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<< '{"tool_name":"Edit","tool_input":{},"session_id":"sess-harness"}'

  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "writes blocked audit row" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" \
    <<<"$(make_edit_input Edit "$HOME/.claude/hooks/guard_allowed_commands.sh")"

  [ "$status" -eq 2 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "Blocked" ]]
  [[ $(echo "$entry" | jq -r '.status') == "blocked" ]]
  [[ $(echo "$entry" | jq -r '.hook') == "guard_harness_files.sh" ]]
  [[ $(echo "$entry" | jq -r '.session') == "sess-harness" ]]
}
