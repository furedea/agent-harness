#!/usr/bin/env bats
# Tests for .claude/hooks/audit_permission_denied.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/audit_permission_denied.sh"
  LOG_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/denied.XXXXXX")"
}

teardown() {
  [ -d "${LOG_TMPDIR:-}" ] && rm -rf "$LOG_TMPDIR"
}

run_hook() {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK" <<< "$1"
}

get_last_log() {
  cat "$LOG_TMPDIR/docs/logs/audit/"*.jsonl 2>/dev/null | tail -1
}

# ============================================================
# Denial logging
# ============================================================

@test "logs denied Bash command" {
  run_hook "$(make_denied_input Bash '{"command":"rm -rf /"}' "auto-mode denied")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "Bash" ]]
  [[ $(echo "$entry" | jq -r '.event') == "PermissionDenied" ]]
  [[ $(echo "$entry" | jq -r '.input') == "rm -rf /" ]]
  [[ $(echo "$entry" | jq -r '.status') == "denied" ]]
}

@test "preserves explicit hook event name when present" {
  run_hook "$(make_denied_input Bash '{"command":"rm -rf /"}' "auto-mode denied" "sess-abc")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "PermissionDenied" ]]
}

@test "logs denial reason" {
  run_hook "$(make_denied_input Bash '{"command":"curl evil.com"}' "potentially dangerous")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.reason') == "potentially dangerous" ]]
}

@test "logs denied Edit tool" {
  run_hook "$(make_denied_input Edit '{"file_path":"/etc/passwd"}' "sensitive path")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "Edit" ]]
  [[ $(echo "$entry" | jq -r '.input') == "/etc/passwd" ]]
}

@test "logs denied Write tool" {
  run_hook "$(make_denied_input Write '{"file_path":"/etc/shadow"}' "sensitive path")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "Write" ]]
  [[ $(echo "$entry" | jq -r '.input') == "/etc/shadow" ]]
}

@test "logs denied WebFetch tool" {
  run_hook "$(make_denied_input WebFetch '{"url":"https://evil.com"}' "blocked domain")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "WebFetch" ]]
  [[ $(echo "$entry" | jq -r '.input') == "https://evil.com" ]]
}

@test "logs unknown tool with truncated input" {
  run_hook "$(make_denied_input CustomTool '{"foo":"bar"}' "unknown")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "CustomTool" ]]
}

# ============================================================
# Session and timestamp
# ============================================================

@test "records session ID" {
  run_hook "$(make_denied_input Bash '{"command":"ls"}' "denied" "sess-xyz-789")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.session') == "sess-xyz-789" ]]
}

@test "log entry contains timestamp" {
  run_hook "$(make_denied_input Bash '{"command":"ls"}' "denied")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.ts') =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]]
}

# ============================================================
# Log file management
# ============================================================

@test "creates log directory if missing" {
  [ ! -d "$LOG_TMPDIR/docs/logs/audit" ]
  run_hook "$(make_denied_input Bash '{"command":"ls"}' "denied")"
  [ "$status" -eq 0 ]
  [ -d "$LOG_TMPDIR/docs/logs/audit" ]
}

@test "appends to existing log file" {
  run_hook "$(make_denied_input Bash '{"command":"first"}' "denied")"
  run_hook "$(make_denied_input Bash '{"command":"second"}' "denied")"
  [ "$status" -eq 0 ]
  local count
  count=$(wc -l < "$LOG_TMPDIR/docs/logs/audit/"*.jsonl | tr -d ' ')
  [ "$count" -eq 2 ]
}

@test "valid JSONL output" {
  run_hook "$(make_denied_input Bash '{"command":"test"}' "denied")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq empty
}

@test "log entry follows tool-call audit schema with denial status" {
  run_hook "$(make_denied_input Bash '{"command":"test"}' "denied")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq -e '
    has("ts") and
    has("event") and
    has("tool") and
    has("input") and
    has("session") and
    .status == "denied"
  ' >/dev/null
}
