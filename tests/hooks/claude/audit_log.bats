#!/usr/bin/env bats
# Tests for .claude/hooks/lib/audit_log.sh::log_blocked

setup() {
  load test-helper/setup
  LIB="$HOOK_DIR/lib/audit_log.sh"
  LOG_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/log.XXXXXX")"
}

teardown() {
  [ -d "${LOG_TMPDIR:-}" ] && rm -rf "$LOG_TMPDIR"
}

call_log_blocked() {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" bash -c '
    source "$1"
    log_blocked "$2" "$3" "$4" "$5" "$6"
  ' _ "$LIB" "$@"
}

get_last_log() {
  cat "$LOG_TMPDIR/docs/logs/audit/"*.jsonl 2>/dev/null | tail -1
}

@test "emits Blocked event row" {
  call_log_blocked Bash "git push --force" "force-push flag detected" guard_dangerous_git.sh sess-1
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "Blocked" ]]
  [[ $(echo "$entry" | jq -r '.status') == "blocked" ]]
}

@test "row preserves tool, input, reason, hook, session" {
  call_log_blocked Bash "rm -rf /" "destructive command" guard_dangerous_git.sh sess-xyz
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.tool') == "Bash" ]]
  [[ $(echo "$entry" | jq -r '.input') == "rm -rf /" ]]
  [[ $(echo "$entry" | jq -r '.reason') == "destructive command" ]]
  [[ $(echo "$entry" | jq -r '.hook') == "guard_dangerous_git.sh" ]]
  [[ $(echo "$entry" | jq -r '.session') == "sess-xyz" ]]
}

@test "row contains UTC timestamp" {
  call_log_blocked Bash "ls" "test" hook.sh sess-1
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.ts') =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]]
}

@test "creates audit log directory if missing" {
  [ ! -d "$LOG_TMPDIR/docs/logs/audit" ]
  call_log_blocked Bash "ls" "test" hook.sh sess-1
  [ -d "$LOG_TMPDIR/docs/logs/audit" ]
}

@test "writes valid JSONL" {
  call_log_blocked Bash "ls" "test" hook.sh sess-1
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq empty
}

@test "blocked row follows shared audit schema" {
  call_log_blocked Bash "ls" "test" hook.sh sess-1
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq -e '
    has("ts") and
    has("event") and
    has("status") and
    has("tool") and
    has("input") and
    has("reason") and
    has("session") and
    .status == "blocked"
  ' >/dev/null
}

@test "appends multiple rows to the same file" {
  call_log_blocked Bash "first" "r1" hook.sh sess-1
  call_log_blocked Bash "second" "r2" hook.sh sess-1
  local count
  count=$(wc -l < "$LOG_TMPDIR/docs/logs/audit/"*.jsonl | tr -d ' ')
  [ "$count" -eq 2 ]
}

@test "tolerates missing session id" {
  call_log_blocked Bash "ls" "test" hook.sh ""
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.session') == "" ]]
}

# ============================================================
# End-to-end: real block hook must emit a Blocked audit row
# ============================================================

@test "guard_allowed_commands.sh emits a Blocked row for no-verify" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK_DIR/guard_allowed_commands.sh" \
    <<<"$(jq -n '{tool_input:{command:"git commit --no-verify -m x"},session_id:"sess-e2e"}')"
  [ "$status" -eq 2 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "Blocked" ]]
  [[ $(echo "$entry" | jq -r '.hook') == "guard_allowed_commands.sh" ]]
  [[ $(echo "$entry" | jq -r '.session') == "sess-e2e" ]]
  [[ $(echo "$entry" | jq -r '.tool') == "Bash" ]]
}

@test "guard_dangerous_git.sh emits a Blocked row when it blocks" {
  CLAUDE_PROJECT_DIR="$LOG_TMPDIR" run bash "$HOOK_DIR/guard_dangerous_git.sh" \
    <<<"$(jq -n '{tool_input:{command:"git push --force origin main"},session_id:"sess-e2e"}')"
  [ "$status" -eq 2 ]
  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "Blocked" ]]
  [[ $(echo "$entry" | jq -r '.hook') == "guard_dangerous_git.sh" ]]
  # First line of the BLOCKED stderr message is preserved as the reason.
  [[ $(echo "$entry" | jq -r '.reason') == BLOCKED:* ]]
}
