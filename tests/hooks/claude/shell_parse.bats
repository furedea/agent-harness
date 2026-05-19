#!/usr/bin/env bats
# Tests for .claude/hooks/lib/shell_parse.sh

setup() {
  load test-helper/setup
  LIB="$HOOK_DIR/lib/shell_parse.sh"
}

split() {
  source "$LIB"
  split_command_segments "$1"
}

# ============================================================
# Single commands (no splitting)
# ============================================================

@test "single command passes through unchanged" {
  run split "git status"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "git status" ]
  [ "${#lines[@]}" -eq 1 ]
}

@test "empty string returns empty line" {
  run split ""
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "" ]
}

# ============================================================
# Pipe splitting
# ============================================================

@test "splits on pipe" {
  run split "ls | grep foo"
  [ "${lines[0]}" = "ls " ]
  [ "${lines[1]}" = " grep foo" ]
}

@test "splits on double pipe (||)" {
  run split "cmd1 || cmd2"
  [ "${lines[0]}" = "cmd1 " ]
  [ "${lines[1]}" = " cmd2" ]
}

@test "splits on triple pipe segments" {
  run split "a | b | c"
  [ "${#lines[@]}" -eq 3 ]
  [ "${lines[0]}" = "a " ]
  [ "${lines[2]}" = " c" ]
}

# ============================================================
# && splitting
# ============================================================

@test "splits on &&" {
  run split "cmd1 && cmd2"
  [ "${lines[0]}" = "cmd1 " ]
  [ "${lines[1]}" = " cmd2" ]
}

@test "splits on chained &&" {
  run split "a && b && c"
  [ "${#lines[@]}" -eq 3 ]
}

# ============================================================
# Semicolon splitting
# ============================================================

@test "splits on semicolon" {
  run split "cmd1; cmd2"
  [ "${lines[0]}" = "cmd1" ]
  [ "${lines[1]}" = " cmd2" ]
}

# ============================================================
# Background & splitting
# ============================================================

@test "splits on background &" {
  run split "cmd1 & cmd2"
  [ "${lines[0]}" = "cmd1 " ]
  [ "${lines[1]}" = " cmd2" ]
}

@test "does not split on & in 2>&1" {
  run split "cmd 2>&1"
  [ "${#lines[@]}" -eq 1 ]
  [ "${lines[0]}" = "cmd 2>&1" ]
}

@test "does not split on & in >&2" {
  run split "cmd >&2"
  [ "${#lines[@]}" -eq 1 ]
  [ "${lines[0]}" = "cmd >&2" ]
}

# ============================================================
# Mixed operators
# ============================================================

@test "splits mixed pipe and &&" {
  run split "a | b && c"
  [ "${#lines[@]}" -eq 3 ]
  [ "${lines[0]}" = "a " ]
}

@test "splits mixed semicolon and ||" {
  run split "a; b || c"
  [ "${#lines[@]}" -eq 3 ]
}

# ============================================================
# Single-quoted strings
# ============================================================

@test "does not split on pipe inside single quotes" {
  run split "echo 'a | b'"
  [ "${#lines[@]}" -eq 1 ]
  [[ "${lines[0]}" == *"a | b"* ]]
}

@test "does not split on && inside single quotes" {
  run split "echo 'a && b'"
  [ "${#lines[@]}" -eq 1 ]
}

@test "does not split on semicolon inside single quotes" {
  run split "echo 'a; b'"
  [ "${#lines[@]}" -eq 1 ]
}

# ============================================================
# Double-quoted strings
# ============================================================

@test "does not split on pipe inside double quotes" {
  run split 'echo "a | b"'
  [ "${#lines[@]}" -eq 1 ]
  [[ "${lines[0]}" == *"a | b"* ]]
}

@test "does not split on && inside double quotes" {
  run split 'echo "a && b"'
  [ "${#lines[@]}" -eq 1 ]
}

# ============================================================
# Backslash escapes
# ============================================================

@test "does not split on escaped pipe" {
  run split 'echo a \| b'
  [ "${#lines[@]}" -eq 1 ]
}

@test "does not split on escaped semicolon" {
  run split 'echo a \; b'
  [ "${#lines[@]}" -eq 1 ]
}

@test "does not split on escaped ampersand" {
  run split 'echo a \& b'
  [ "${#lines[@]}" -eq 1 ]
}

# ============================================================
# Complex realistic commands
# ============================================================

@test "git commit with quoted message then push" {
  run split "git commit -m 'fix: update' && git push origin main"
  [ "${#lines[@]}" -eq 2 ]
  [[ "${lines[0]}" == *"git commit"* ]]
  [[ "${lines[1]}" == *"git push"* ]]
}

@test "pipe chain with quoted argument" {
  run split "grep 'pattern | other' file.txt | wc -l"
  [ "${#lines[@]}" -eq 2 ]
  [[ "${lines[0]}" == *"pattern | other"* ]]
  [[ "${lines[1]}" == *"wc -l"* ]]
}
