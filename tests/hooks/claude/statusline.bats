#!/usr/bin/env bats
# Tests for .claude/statusline/statusline.sh

setup() {
  load test-helper/setup
  STATUSLINE="$REPO_ROOT/claude/statusline/statusline.sh"
}

# Build a minimal statusline JSON input.
# Usage: make_statusline_input '{"model":{"display_name":"Opus"},...}'
make_statusline_input() {
  printf '%s' "$1"
}

MINIMAL_INPUT='{"model":{"display_name":"Opus 4.6"},"cwd":"/tmp/test","context_window":{"used_percentage":25}}'

# ============================================================
# File structure
# ============================================================

@test "statusline.sh exists and is executable" {
  [ -f "$STATUSLINE" ]
  [ -x "$STATUSLINE" ]
}

@test "statusline.sh passes bash syntax check" {
  run bash -n "$STATUSLINE"
  [ "$status" -eq 0 ]
}

@test "statusline.sh uses /bin/bash shebang" {
  head -1 "$STATUSLINE" | grep -q '#!/bin/bash'
}

# ============================================================
# Basic output
# ============================================================

@test "produces two lines of output" {
  run bash "$STATUSLINE" <<< "$MINIMAL_INPUT"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 2 ]
}

@test "line 1 contains model name" {
  run bash "$STATUSLINE" <<< "$MINIMAL_INPUT"
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == *"Opus 4.6"* ]]
}

@test "line 2 contains context percentage" {
  run bash "$STATUSLINE" <<< "$MINIMAL_INPUT"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"25%"* ]]
}

@test "line 2 contains Ctx label" {
  run bash "$STATUSLINE" <<< "$MINIMAL_INPUT"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"Ctx:"* ]]
}

# ============================================================
# CWD shortening
# ============================================================

@test "shortens long CWD to current directory name" {
  local input='{"model":{"display_name":"Opus"},"cwd":"/a/b/c/d/e","context_window":{"used_percentage":0}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == *"e"* ]]
  [[ "${lines[0]}" != *"c/d/e"* ]]
}

# ============================================================
# Rate limits
# ============================================================

@test "shows 5h rate limit when provided" {
  local input='{"model":{"display_name":"Opus"},"cwd":"/tmp","context_window":{"used_percentage":10},"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":null}}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"42%"* ]]
  [[ "${lines[1]}" == *"5h:"* ]]
}

@test "shows 7d rate limit when provided" {
  local input='{"model":{"display_name":"Opus"},"cwd":"/tmp","context_window":{"used_percentage":10},"rate_limits":{"seven_day":{"used_percentage":15,"resets_at":null}}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"15%"* ]]
  [[ "${lines[1]}" == *"7d:"* ]]
}

@test "omits 5h section when not present" {
  run bash "$STATUSLINE" <<< "$MINIMAL_INPUT"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" != *"5h:"* ]]
}

# ============================================================
# Edge cases
# ============================================================

@test "handles empty model name" {
  local input='{"model":{},"cwd":"/tmp","context_window":{"used_percentage":50}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 2 ]
}

@test "handles 0% context" {
  local input='{"model":{"display_name":"Opus"},"cwd":"/tmp","context_window":{"used_percentage":0}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"0%"* ]]
}

@test "handles 100% context" {
  local input='{"model":{"display_name":"Opus"},"cwd":"/tmp","context_window":{"used_percentage":100}}'
  run bash "$STATUSLINE" <<< "$input"
  [ "$status" -eq 0 ]
  [[ "${lines[1]}" == *"100%"* ]]
}

# ============================================================
# Helper functions
# ============================================================

@test "fmt_duration formats days" {
  # Extract fmt_duration function and test it in isolation
  run bash -c "$(sed -n '/^function fmt_duration()/,/^}/p' "$STATUSLINE"); fmt_duration 90061"
  [[ "$output" == "1d 1h" ]]
}

@test "fmt_duration formats hours" {
  run bash -c "$(sed -n '/^function fmt_duration()/,/^}/p' "$STATUSLINE"); fmt_duration 7260"
  [[ "$output" == "2h 1m" ]]
}

@test "fmt_duration formats minutes" {
  run bash -c "$(sed -n '/^function fmt_duration()/,/^}/p' "$STATUSLINE"); fmt_duration 300"
  [[ "$output" == "5m" ]]
}
