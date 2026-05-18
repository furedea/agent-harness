#!/bin/bash
# Claude Code PermissionDenied hook: append auto-mode classifier denials to the audit log.
#
# Fires only when the auto-mode classifier denies a tool call. Does NOT fire for:
#   - PreToolUse hook denials (each hook must log its own denials)
#   - permissions.deny rule matches
#   - Manual permission-dialog denials
#
# Writes one JSONL line per denial to docs/logs/audit/YYYY-MM-DD.jsonl.
# Logging failures are silent (exit 0).

set -u

INPUT=$(cat)

if ! command -v jq >/dev/null 2>&1; then
  cat >&2 <<ERRMSG
ERROR: jq is not installed.

Why: This hook requires jq to parse tool input JSON and format the audit log.

What to do:
  Claude Code: Ask the user to install jq.
  User: Install jq (e.g., brew install jq on macOS, sudo apt-get install jq on Linux).
ERRMSG
  exit 2
fi

EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // "PermissionDenied"' 2>/dev/null || true)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null || true)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
REASON=$(echo "$INPUT" | jq -r '.reason // empty' 2>/dev/null || true)

case "$TOOL" in
  Bash)
    SUMMARY=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
    ;;
  Edit | Write | MultiEdit | NotebookEdit)
    SUMMARY=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)
    ;;
  WebFetch)
    SUMMARY=$(echo "$INPUT" | jq -r '.tool_input.url // empty' 2>/dev/null || true)
    ;;
  *)
    SUMMARY=$(echo "$INPUT" | jq -rc '.tool_input // {}' 2>/dev/null | head -c 200 || true)
    ;;
esac

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
LOG_DIR="$PROJECT_DIR/docs/logs/audit"
mkdir -p "$LOG_DIR" 2>/dev/null || true
LOG_FILE="$LOG_DIR/$(date -u +%Y-%m-%d).jsonl"

TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

jq -cn --arg ts "$TS" --arg event "$EVENT" --arg tool "$TOOL" --arg input "$SUMMARY" \
  --arg session "$SESSION" --arg reason "$REASON" \
  '{ts: $ts, event: $event, tool: $tool, status: "denied", input: $input, reason: $reason, session: $session}' \
  >>"$LOG_FILE" 2>/dev/null || true

exit 0
