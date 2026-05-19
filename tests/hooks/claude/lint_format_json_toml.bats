#!/usr/bin/env bats
# Behavioral tests for lint_format_json_toml.sh under the Quality Loop model:
# format via dprint first, then emit residual dprint check diagnostics via
# hookSpecificOutput.additionalContext JSON. Always exits 0.

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/lint_format_json_toml.sh"
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/lintjson.XXXXXX")"
}

teardown() {
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

make_input_file() {
  local _file_path="$1"
  local _input_file="$TEST_TMPDIR/input.json"

  jq -n --arg file_path "$_file_path" '{"tool_input":{"file_path":$file_path}}' >"$_input_file"
  echo "$_input_file"
}

write_dprint_stub() {
  local _check_status="$1"
  local _check_output="${2:-}"

  cat >"$TEST_TMPDIR/dprint" <<EOF
#!/bin/bash
set -eo pipefail

case "\$1" in
  fmt)
    exit 0
    ;;
  check)
    if [ -n "$_check_output" ]; then
      printf '%s\n' "$_check_output"
    fi
    exit $_check_status
    ;;
  *)
    exit 1
    ;;
esac
EOF
  chmod +x "$TEST_TMPDIR/dprint"
}

@test "lint_format_json_toml emits no JSON when dprint check passes after format" {
  write_dprint_stub 0
  local _file="$TEST_TMPDIR/config.json"
  printf '{"ok":true}\n' >"$_file"
  local _input
  _input="$(make_input_file "$_file")"

  PATH="$TEST_TMPDIR:$PATH" HOME="$REPO_ROOT" run bash -c 'bash "$1" < "$2"' _ "$HOOK" "$_input"

  [ "$status" -eq 0 ]
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_json_toml emits PostToolUse JSON for residual dprint diagnostics" {
  write_dprint_stub 1 "config.json is not formatted"
  local _file="$TEST_TMPDIR/config.json"
  printf '{"ok":true}\n' >"$_file"
  local _input
  _input="$(make_input_file "$_file")"

  PATH="$TEST_TMPDIR:$PATH" HOME="$REPO_ROOT" run bash -c 'bash "$1" < "$2"' _ "$HOOK" "$_input"

  [ "$status" -eq 0 ]
  [[ "$output" == *"hookSpecificOutput"* ]]
  local _event
  _event="$(echo "$output" | jq -r '.hookSpecificOutput.hookEventName')"
  [ "$_event" = "PostToolUse" ]
  local _ctx
  _ctx="$(echo "$output" | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$_ctx" == *"dprint"* ]]
  [[ "$_ctx" == *"config.json is not formatted"* ]]
}
