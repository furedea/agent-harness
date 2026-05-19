#!/bin/bash
# Claude Code PreToolUse hook: block edits to the agent harness itself.
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
    "~/.codex/hooks/"* | "~/.codex/hooks.json" | "~/.codex/AGENTS.md" | "~/.codex/rules/default.rules" | \
    */agent-harness/agents/hooks/* | */agent-harness/agents/AGENTS.md | \
    */agent-harness/codex/hooks/* | */agent-harness/codex/hooks.json | \
    */dotfiles/agents/hooks/* | */dotfiles/agents/AGENTS.md | \
    */dotfiles/codex/hooks/* | */dotfiles/codex/hooks.json)
    log_blocked "$TOOL" "$FILE_PATH" "agent harness boundary is protected" guard_harness_files.sh "$SESSION"
    cat >&2 <<ERRMSG
BLOCKED: $FILE_PATH is part of the agent harness boundary.

Why: Hooks, agent instructions, and generated permission bindings protect the
     safety checks themselves. Change the calling code or tests instead of
     weakening the harness from inside an agent run.

What to do:
  Claude Code: Stop and ask the user to make or explicitly authorize this
               harness change.
  User: Edit the harness manually or temporarily adjust permissions outside
        the protected agent session.
ERRMSG
    exit 2
    ;;
esac

exit 0
