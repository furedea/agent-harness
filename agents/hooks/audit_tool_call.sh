#!/bin/bash
# Claude Code PreToolUse + PostToolUse hook: append every tool call to the audit log.
#
# Writes one JSONL line per call to docs/logs/audit/YYYY-MM-DD.jsonl.
# The script is event-agnostic — it reads `.hook_event_name` from stdin and
# stamps the same value into the `event` field, so the operator can tell
# intent (PreToolUse) apart from result (PostToolUse) when reviewing the
# audit log. Registering this hook FIRST in each PreToolUse matcher captures
# intents that subsequent gating hooks short-circuit with exit 2.
#
# Records: timestamp, event, status, tool, input summary, reason, session.
# Does NOT record: file contents, command stdout/stderr.
#
# Logging failures are silent (exit 0) — audit is observability, not enforcement.

set -u

INPUT=$(cat)

# jq is required; if missing, silently skip.
command -v jq >/dev/null 2>&1 || exit 0

EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // "PostToolUse"' 2>/dev/null || true)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null || true)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
[ -z "$TOOL" ] && exit 0

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
  Agent)
    SUMMARY=$(echo "$INPUT" | jq -r '(.tool_input.subagent_type // "general-purpose") + ": " + (.tool_input.description // "")' 2>/dev/null || true)
    ;;
  *)
    SUMMARY=$(echo "$INPUT" | jq -rc '.tool_input // {}' 2>/dev/null | head -c 200 || true)
    ;;
esac

[ -z "$SUMMARY" ] && exit 0

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
LOG_DIR="$PROJECT_DIR/docs/logs/audit"
mkdir -p "$LOG_DIR" 2>/dev/null || true
LOG_FILE="$LOG_DIR/$(date -u +%Y-%m-%d).jsonl"

TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

jq -cn --arg ts "$TS" --arg event "$EVENT" --arg tool "$TOOL" --arg input "$SUMMARY" --arg session "$SESSION" \
  '{ts: $ts, event: $event, status: "observed", tool: $tool, input: $input, reason: "", session: $session}' \
  >>"$LOG_FILE" 2>/dev/null || true

exit 0
