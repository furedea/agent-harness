#!/usr/bin/env bats
# Behavioral tests for lint_format_py.sh under the Quality Loop model:
# format + auto-fix first, then emit residual lint violations via
# hookSpecificOutput.additionalContext JSON. Hook always exits 0.

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/lint_format_py.sh"
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/lintpy.XXXXXX")"
}

teardown() {
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

@test "lint_format_py exits 0 on a clean file with no output JSON" {
  local _file="$TEST_TMPDIR/clean.py"
  printf 'x = 1\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_py auto-fixes an unused import and emits no JSON" {
  local _file="$TEST_TMPDIR/unused_import.py"
  printf 'import os\nx = 1\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  ! grep -q '^import os' "$_file"
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_py auto-formats a malformatted file and emits no JSON" {
  local _file="$TEST_TMPDIR/badfmt.py"
  printf 'x=1\ny  =   2\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  grep -q '^x = 1$' "$_file"
  grep -q '^y = 2$' "$_file"
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_py emits PostToolUse JSON for non-auto-fixable violation" {
  local _file="$TEST_TMPDIR/undefined.py"
  printf 'def f():\n    return undefined_name\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  [[ "$output" == *"hookSpecificOutput"* ]]
  local _event
  _event="$(echo "$output" | grep '"hookSpecificOutput"' | jq -r '.hookSpecificOutput.hookEventName')"
  [ "$_event" = "PostToolUse" ]
  local _ctx
  _ctx="$(echo "$output" | grep '"hookSpecificOutput"' | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$_ctx" == *"F821"* ]] || [[ "$_ctx" == *"undefined"* ]]
}

@test "lint_format_py exits 0 when input has no file_path" {
  run bash "$HOOK" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}
