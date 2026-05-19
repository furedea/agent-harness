#!/usr/bin/env bats
# Tests for .claude/hooks/audit_compaction.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/audit_compaction.sh"
  LOG_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/log.XXXXXX")"
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

# A transcript with a known shape so the test can predict counts.
# Lines (4): user / assistant (1 tool_use) / user / assistant (2 tool_uses) → 3 tool_uses total.
write_sample_transcript() {
  local path="$1"
  cat >"$path" <<'EOF'
{"type":"user","message":{"content":[{"type":"text","text":"hi"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"ok"},{"type":"tool_use","name":"Bash","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{}},{"type":"tool_use","name":"Write","input":{}}]}}
EOF
}

# ============================================================
# JSONL row shape
# ============================================================

@test "emits one JSONL row with event=PreCompact" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input manual sess-1 "$transcript")"
  [ "$status" -eq 0 ]

  local entry count
  count=$(wc -l < "$LOG_TMPDIR/docs/logs/audit/"*.jsonl | tr -d ' ')
  [ "$count" -eq 1 ]
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "PreCompact" ]]
  [[ $(echo "$entry" | jq -r '.status') == "observed" ]]
  [[ $(echo "$entry" | jq -r '.tool') == "Compaction" ]]
  [[ $(echo "$entry" | jq -r '.reason') == "" ]]
}

@test "row preserves trigger, session and transcript_path" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input auto sess-xyz "$transcript")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.trigger') == "auto" ]]
  [[ $(echo "$entry" | jq -r '.session') == "sess-xyz" ]]
  [[ $(echo "$entry" | jq -r '.transcript_path') == "$transcript" ]]
}

@test "row contains UTC timestamp" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input manual sess-1 "$transcript")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.ts') =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]]
}

# ============================================================
# Transcript counts
# ============================================================

@test "counts messages and tool_uses from transcript" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input manual sess-1 "$transcript")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [ "$(echo "$entry" | jq -r '.counts.messages')" -eq 4 ]
  [ "$(echo "$entry" | jq -r '.counts.tool_uses')" -eq 3 ]
}

# ============================================================
# Edge cases
# ============================================================

@test "defaults counts to zero when transcript_path is empty" {
  run_hook "$(make_compact_input manual sess-1 "")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [ "$(echo "$entry" | jq -r '.counts.messages')" -eq 0 ]
  [ "$(echo "$entry" | jq -r '.counts.tool_uses')" -eq 0 ]
}

@test "defaults counts to zero when transcript file is missing" {
  run_hook "$(make_compact_input manual sess-1 "$LOG_TMPDIR/missing.jsonl")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [ "$(echo "$entry" | jq -r '.counts.messages')" -eq 0 ]
  [ "$(echo "$entry" | jq -r '.counts.tool_uses')" -eq 0 ]
}

@test "creates audit log directory if missing" {
  [ ! -d "$LOG_TMPDIR/docs/logs/audit" ]
  run_hook "$(make_compact_input manual sess-1 "")"
  [ "$status" -eq 0 ]
  [ -d "$LOG_TMPDIR/docs/logs/audit" ]
}

@test "emits valid JSONL" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input manual sess-1 "$transcript")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq empty
}

@test "row follows shared audit schema" {
  local transcript="$LOG_TMPDIR/transcript.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_compact_input manual sess-1 "$transcript")"
  [ "$status" -eq 0 ]
  local entry
  entry=$(get_last_log)
  echo "$entry" | jq -e '
    has("ts") and
    has("event") and
    has("status") and
    has("tool") and
    has("input") and
    has("reason") and
    has("session")
  ' >/dev/null
}

# ============================================================
# SessionStart — post-compaction counts (paired with PreCompact rows)
# ============================================================

@test "SessionStart with source=compact emits a SessionStart row with post-compaction counts" {
  # Smaller transcript than the pre-compact one to simulate context loss.
  local transcript="$LOG_TMPDIR/post.jsonl"
  cat >"$transcript" <<'EOF'
{"type":"user","message":{"content":[{"type":"text","text":"resumed"}]}}
EOF

  run_hook "$(make_session_start_input compact sess-1 "$transcript")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "SessionStart" ]]
  [[ $(echo "$entry" | jq -r '.source') == "compact" ]]
  [ "$(echo "$entry" | jq -r '.counts.messages')" -eq 1 ]
  [ "$(echo "$entry" | jq -r '.counts.tool_uses')" -eq 0 ]
}

@test "SessionStart with source=resume also records counts" {
  local transcript="$LOG_TMPDIR/resumed.jsonl"
  write_sample_transcript "$transcript"

  run_hook "$(make_session_start_input resume sess-1 "$transcript")"
  [ "$status" -eq 0 ]

  local entry
  entry=$(get_last_log)
  [[ $(echo "$entry" | jq -r '.event') == "SessionStart" ]]
  [[ $(echo "$entry" | jq -r '.source') == "resume" ]]
  [ "$(echo "$entry" | jq -r '.counts.messages')" -eq 4 ]
}

@test "SessionStart with source=startup is ignored (no row written)" {
  # Brand new sessions have no relationship to PreCompact context loss;
  # logging them would just noise up the audit feed.
  run_hook "$(make_session_start_input startup sess-1 "")"
  [ "$status" -eq 0 ]
  [ ! -f "$LOG_TMPDIR/docs/logs/audit/$(date -u +%Y-%m-%d).jsonl" ]
}
