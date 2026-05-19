#!/usr/bin/env bats
# Behavioral tests for lint_format_js.sh under the Quality Loop model:
# format (oxfmt) + auto-fix (oxlint --fix) first, then emit residual
# violations via hookSpecificOutput.additionalContext JSON. Always exits 0.

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/lint_format_js.sh"
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/lintjs.XXXXXX")"
}

teardown() {
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

@test "lint_format_js exits 0 on a clean file with no output JSON" {
  local _file="$TEST_TMPDIR/clean.js"
  printf 'const x = 1;\nconsole.log(x);\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_js auto-formats a malformatted file and emits no JSON" {
  local _file="$TEST_TMPDIR/badfmt.js"
  printf 'const   x=1\nconst y  =  2;\nconsole.log(x,y);\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  grep -q '^const x = 1;' "$_file"
}

@test "lint_format_js emits PostToolUse JSON for non-auto-fixable warning" {
  local _file="$TEST_TMPDIR/unused.js"
  printf 'var x = 1;\nvar x = 2;\n' > "$_file"
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  [[ "$output" == *"hookSpecificOutput"* ]]
  local _event
  _event="$(echo "$output" | grep '"hookSpecificOutput"' | jq -r '.hookSpecificOutput.hookEventName')"
  [ "$_event" = "PostToolUse" ]
}

@test "lint_format_js exits 0 when input has no file_path" {
  run bash "$HOOK" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}
