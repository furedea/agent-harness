#!/bin/bash
# Claude Code PreToolUse hook: block edits to installed harness files.
# The permissions/sandbox layer is the hard boundary; this hook adds an
# explanatory block reason plus audit logging before that boundary is reached.
# Exit code 0 = allow, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // "Edit"')
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty')
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // empty')

[ -z "$FILE_PATH" ] && exit 0

# shellcheck disable=SC2088  # literal "~/" patterns are matched intentionally
case "$FILE_PATH" in
"$HOME/.claude/hooks/"* | "$HOME/.claude/settings.json" | "$HOME/.claude/CLAUDE.md" | \
  "~/.claude/hooks/"* | "~/.claude/settings.json" | "~/.claude/CLAUDE.md" | \
  "$HOME/.codex/hooks/"* | "$HOME/.codex/hooks.json" | "$HOME/.codex/AGENTS.md" | "$HOME/.codex/rules/default.rules" | \
  "~/.codex/hooks/"* | "~/.codex/hooks.json" | "~/.codex/AGENTS.md" | "~/.codex/rules/default.rules")
  log_blocked "$TOOL" "$FILE_PATH" "agent harness boundary is protected" guard_harness_files.sh "$SESSION"
  cat >&2 <<ERRMSG
BLOCKED: $FILE_PATH is part of the agent harness boundary.

Why: Installed hooks, agent instructions, and generated permission bindings
     protect the safety checks themselves. Change the agent-harness source and
     regenerate these files instead of editing generated output.

What to do:
  Claude Code: Change the agent-harness source, then regenerate the installed
               files.
  User: Review and authorize the source change as usual.
ERRMSG
  exit 2
  ;;
esac

exit 0
