#!/bin/bash
# Claude Code PreCompact + SessionStart hook: record how much conversation
# context is lost across the compaction boundary.
#
# Compaction drops earlier turns; without an external record the operator
# cannot reconstruct what the agent had been doing when the boundary fired.
# Two events bracket the boundary:
#   - PreCompact:                  counts BEFORE compaction
#   - SessionStart(source=compact) counts AFTER compaction (also source=resume)
#
# Pairing the rows on `session_id` makes "messages_before - messages_after"
# the volume of context that was elided. The `source=startup` case is
# deliberately dropped — fresh sessions are unrelated to compaction loss
# and would just noise up the feed.
#
# Records: timestamp, event, status, input, reason, session, transcript_path,
# counts, and trigger|source.
# Does NOT record: transcript bodies (the file already lives on disk).
#
# Logging failures are silent (exit 0) — observability, not enforcement.

set -u

INPUT=$(cat)

command -v jq >/dev/null 2>&1 || exit 0

EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // "PreCompact"' 2>/dev/null || true)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
TRANSCRIPT=$(echo "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null || true)
TRIGGER=""
SOURCE=""

case "$EVENT" in
  PreCompact)
    TRIGGER=$(echo "$INPUT" | jq -r '.trigger // empty' 2>/dev/null || true)
    ;;
  SessionStart)
    SOURCE=$(echo "$INPUT" | jq -r '.source // empty' 2>/dev/null || true)
    # Only `compact` and `resume` carry pre-existing context worth pairing.
    # `startup` (and any future kinds) are skipped so the audit feed stays
    # focused on the compaction boundary.
    case "$SOURCE" in
      compact | resume) ;;
      *) exit 0 ;;
    esac
    ;;
  *)
    exit 0
    ;;
esac

MESSAGES=0
TOOL_USES=0
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
  MESSAGES=$(wc -l <"$TRANSCRIPT" 2>/dev/null | tr -d ' ' || echo 0)
  # Stream the JSONL line-by-line. Each line that has a `tool_use` block in
  # `.message.content[]` emits one "1" per tool_use; `wc -l` totals them.
  TOOL_USES=$(jq -r '.message?.content?[]? | select(.type == "tool_use") | "1"' "$TRANSCRIPT" 2>/dev/null | wc -l | tr -d ' ' || echo 0)
fi

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
LOG_DIR="$PROJECT_DIR/docs/logs/audit"
mkdir -p "$LOG_DIR" 2>/dev/null || true
LOG_FILE="$LOG_DIR/$(date -u +%Y-%m-%d).jsonl"

TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

jq -cn \
  --arg ts "$TS" \
  --arg event "$EVENT" \
  --arg status "observed" \
  --arg input "$TRANSCRIPT" \
  --arg reason "" \
  --arg trigger "$TRIGGER" \
  --arg source "$SOURCE" \
  --arg session "$SESSION" \
  --arg path "$TRANSCRIPT" \
  --argjson messages "${MESSAGES:-0}" \
  --argjson tool_uses "${TOOL_USES:-0}" \
  '{ts: $ts, event: $event, status: $status, tool: "Compaction", input: $input, reason: $reason, trigger: $trigger, source: $source, session: $session, transcript_path: $path, counts: {messages: $messages, tool_uses: $tool_uses}}' \
  >>"$LOG_FILE" 2>/dev/null || true

exit 0
