#!/usr/bin/env bats
# Behavioral tests for lint_format_sh.sh under the Quality Loop model:
# format (shfmt -w) first, then capture residual shellcheck diagnostics
# via hookSpecificOutput.additionalContext JSON. Always exits 0.

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/lint_format_sh.sh"
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/lintsh.XXXXXX")"
}

teardown() {
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

@test "lint_format_sh exits 0 on a clean script with no output JSON" {
  local _file="$TEST_TMPDIR/clean.sh"
  cat > "$_file" <<'EOF'
#!/bin/bash
set -eo pipefail
echo "hello"
EOF
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "lint_format_sh auto-formats a malformatted script and emits no JSON" {
  local _file="$TEST_TMPDIR/badfmt.sh"
  cat > "$_file" <<'EOF'
#!/bin/bash
set -eo pipefail
if true
then
echo "x"
fi
EOF
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  grep -q '^if true; then$' "$_file"
}

@test "lint_format_sh emits PostToolUse JSON for shellcheck violation" {
  local _file="$TEST_TMPDIR/unquoted.sh"
  cat > "$_file" <<'EOF'
#!/bin/bash
echo $1
EOF
  run bash "$HOOK" <<< "$(make_post_tool_input "$_file")"
  [ "$status" -eq 0 ]
  [[ "$output" == *"hookSpecificOutput"* ]]
  local _ctx
  _ctx="$(echo "$output" | grep '"hookSpecificOutput"' | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$_ctx" == *"SC2086"* ]] || [[ "$_ctx" == *"quote"* ]]
}

@test "lint_format_sh exits 0 when input has no file_path" {
  run bash "$HOOK" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}
