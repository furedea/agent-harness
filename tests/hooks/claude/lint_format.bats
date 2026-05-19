#!/usr/bin/env bats
# Tests for .claude/hooks/lib/lint_format.sh (shared utilities)

setup() {
  load test-helper/setup
  LIB="$HOOK_DIR/lib/lint_format.sh"
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/lint.XXXXXX")"
}

teardown() {
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

# ============================================================
# load_file_path
# ============================================================

@test "load_file_path sets FILE_PATH from JSON input" {
  printf 'hello\n' > "$TEST_TMPDIR/test.py"
  run bash -c "
    source '$LIB'
    load_file_path <<< '{\"tool_input\":{\"file_path\":\"$TEST_TMPDIR/test.py\"}}'
    echo \"\$FILE_PATH\"
  "
  [ "$status" -eq 0 ]
  [[ "$output" == "$TEST_TMPDIR/test.py" ]]
}

@test "load_file_path sets FILENAME (basename)" {
  printf 'hello\n' > "$TEST_TMPDIR/my_module.py"
  run bash -c "
    source '$LIB'
    load_file_path <<< '{\"tool_input\":{\"file_path\":\"$TEST_TMPDIR/my_module.py\"}}'
    echo \"\$FILENAME\"
  "
  [ "$status" -eq 0 ]
  [[ "$output" == "my_module.py" ]]
}

@test "load_file_path exits 0 silently when no file_path" {
  run bash -c "
    source '$LIB'
    load_file_path <<< '{\"tool_input\":{}}'
    echo 'should not reach here'
  "
  [ "$status" -eq 0 ]
  [[ "$output" != *"should not reach here"* ]]
}

@test "load_file_path exits 1 when file does not exist" {
  run bash -c "
    source '$LIB'
    load_file_path <<< '{\"tool_input\":{\"file_path\":\"$TEST_TMPDIR/nonexistent.py\"}}'
  "
  [ "$status" -eq 1 ]
  [[ "$output" == *"File not found"* ]]
}

# ============================================================
# find_project_root
# ============================================================

@test "find_project_root finds pyproject.toml" {
  mkdir -p "$TEST_TMPDIR/project/src/pkg"
  touch "$TEST_TMPDIR/project/pyproject.toml"
  run bash -c "
    source '$LIB'
    find_project_root '$TEST_TMPDIR/project/src/pkg' pyproject.toml
  "
  [ "$status" -eq 0 ]
  [[ "$output" == "$TEST_TMPDIR/project" ]]
}

@test "find_project_root finds uv.lock" {
  mkdir -p "$TEST_TMPDIR/proj/deep/nested"
  touch "$TEST_TMPDIR/proj/uv.lock"
  run bash -c "
    source '$LIB'
    find_project_root '$TEST_TMPDIR/proj/deep/nested' uv.lock
  "
  [ "$status" -eq 0 ]
  [[ "$output" == "$TEST_TMPDIR/proj" ]]
}

@test "find_project_root checks multiple targets" {
  mkdir -p "$TEST_TMPDIR/proj2/src"
  touch "$TEST_TMPDIR/proj2/Cargo.toml"
  run bash -c "
    source '$LIB'
    find_project_root '$TEST_TMPDIR/proj2/src' pyproject.toml Cargo.toml
  "
  [ "$status" -eq 0 ]
  [[ "$output" == "$TEST_TMPDIR/proj2" ]]
}

@test "find_project_root returns 1 when no marker found" {
  mkdir -p "$TEST_TMPDIR/empty/deep"
  run bash -c "
    source '$LIB'
    find_project_root '$TEST_TMPDIR/empty/deep' nonexistent_marker_file.xyz
  "
  [ "$status" -eq 1 ]
  [ -z "$output" ]
}

# ============================================================
# require_cmd
# ============================================================

@test "require_cmd succeeds for existing command" {
  run bash -c "
    source '$LIB'
    require_cmd bash
  "
  [ "$status" -eq 0 ]
}

@test "require_cmd emits PostToolUse context for nonexistent command" {
  run bash -c "
    source '$LIB'
    require_cmd definitely_not_a_real_command_xyz
  "
  [ "$status" -eq 0 ]
  [[ "$output" == *"not found"* ]]
  [ "$(echo "$output" | jq -r '.hookSpecificOutput.hookEventName')" = "PostToolUse" ]
  local ctx
  ctx="$(echo "$output" | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$ctx" == *"definitely_not_a_real_command_xyz"* ]]
}

# ============================================================
# emit_post_tool_context
# ============================================================

@test "emit_post_tool_context produces JSON with PostToolUse event name when violations present" {
  run bash -c "
    source '$LIB'
    emit_post_tool_context 'ruff' 'F401 unused import'
  "
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.hookSpecificOutput.hookEventName')" = "PostToolUse" ]
}

@test "emit_post_tool_context includes tool label and violations in additionalContext" {
  run bash -c "
    source '$LIB'
    emit_post_tool_context 'ruff' 'F401 unused import'
  "
  [ "$status" -eq 0 ]
  local ctx
  ctx="$(echo "$output" | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$ctx" == *"ruff"* ]]
  [[ "$ctx" == *"F401 unused import"* ]]
}

@test "emit_post_tool_context outputs nothing when violations are empty" {
  run bash -c "
    source '$LIB'
    emit_post_tool_context 'ruff' ''
  "
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "emit_post_tool_context preserves newlines in violations via jq escaping" {
  local input
  input="$(printf 'line1\nline2\nline3')"
  run bash -c "
    source '$LIB'
    emit_post_tool_context 'oxlint' \"$input\"
  "
  [ "$status" -eq 0 ]
  local ctx
  ctx="$(echo "$output" | jq -r '.hookSpecificOutput.additionalContext')"
  [[ "$ctx" == *"line1"* ]]
  [[ "$ctx" == *"line2"* ]]
  [[ "$ctx" == *"line3"* ]]
}
