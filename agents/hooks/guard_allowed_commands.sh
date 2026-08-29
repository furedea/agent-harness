#!/bin/bash
# Claude Code PreToolUse hook: regex-based allowlist for Bash commands needing precise control.
# Splits compound commands (|, ||, &&, ;, &) and validates each segment independently.
# Governed segments must match an allowed pattern; non-governed segments pass through.
# Exit code 0 = allow/pass-through, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/shell_parse.sh"
# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"
# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/command_rules.sh"

HOOK_DIR="$(dirname "${BASH_SOURCE[0]}")"
readonly HOOK_DIR
readonly DEFAULT_COMMAND_PERMISSIONS_FILE="$HOOK_DIR/rules/command_permissions.json"
readonly DEFAULT_ALLOWED_RULES_FILE="$HOOK_DIR/rules/allowed_commands.json"
readonly COMMAND_PERMISSIONS_FILE="${AGENT_COMMAND_PERMISSIONS:-$DEFAULT_COMMAND_PERMISSIONS_FILE}"
readonly ALLOWED_RULES_FILE="${AGENT_ALLOWED_COMMAND_RULES:-$DEFAULT_ALLOWED_RULES_FILE}"

# Require jq for JSON parsing.
if ! command -v jq >/dev/null 2>&1; then
  cat >&2 <<ERRMSG
BLOCKED: jq is not installed.

Why: This hook requires jq to parse tool input JSON. Without it, commands cannot be validated.

What to do:
  Claude Code: Ask the user to install jq.
  User: Install jq (e.g., brew install jq on macOS, sudo apt-get install jq on Linux).
ERRMSG
  exit 2
fi

if [ ! -f "$COMMAND_PERMISSIONS_FILE" ] || ! command_rules_validate_prefix_file "$COMMAND_PERMISSIONS_FILE"; then
  echo "BLOCKED: invalid command permissions: $COMMAND_PERMISSIONS_FILE" >&2
  exit 2
fi
if [ ! -f "$ALLOWED_RULES_FILE" ] || ! command_rules_validate_regex_file "$ALLOWED_RULES_FILE"; then
  echo "BLOCKED: invalid global allowed command rules: $ALLOWED_RULES_FILE" >&2
  exit 2
fi

INPUT=$(cat)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
if ! RAW_COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null); then
  cat >&2 <<ERRMSG
BLOCKED: failed to parse tool input JSON.

Why: The hook received invalid JSON input and cannot validate the command.

What to do:
  Claude Code: Report this error to the user — it may indicate a Claude Code bug or misconfigured hook.
  User: Check that .claude/hooks/guard_allowed_commands.sh is correctly registered in settings.json.
ERRMSG
  exit 2
fi

# Normalize a command segment: trim whitespace and strip trailing shell redirections.
function normalize_segment() {
  echo "$1" | sed -E 's/^[[:space:]]+|[[:space:]]+$//; s/[[:space:]]+(2>&1|2>\/dev\/null|>&2)[[:space:]]*$//'
}

PROJECT_RULES_FILE=$(command_rules_project_file allowed_commands.json || true)
if [ -n "$PROJECT_RULES_FILE" ] && [ -f "$PROJECT_RULES_FILE" ] &&
  ! command_rules_validate_regex_file "$PROJECT_RULES_FILE"; then
  echo "BLOCKED: invalid project allowed command rules: $PROJECT_RULES_FILE" >&2
  exit 2
fi

# Shared command prefixes come from the generated command permissions.

# Precise global forms come from rules/allowed_commands.json.

# Validate each segment of the pipeline independently.
# Governed segments must match an allowed pattern; non-governed segments pass through.
BLOCKED_SEGMENT=""
BLOCKED_REASON=""
while IFS= read -r segment; do
  segment=$(normalize_segment "$segment")
  [ -z "$segment" ] && continue

  if ! command_rules_prefix_reason "$segment" "$COMMAND_PERMISSIONS_FILE" allow >/dev/null; then
    continue
  fi

  # Governed segment: must match an allowed pattern
  segment_allowed=false
  if command_rules_regex_reason "$segment" "$ALLOWED_RULES_FILE" >/dev/null; then
    segment_allowed=true
  fi

  if [ "$segment_allowed" = false ]; then
    if [ -n "$PROJECT_RULES_FILE" ] &&
      command_rules_regex_reason "$segment" "$PROJECT_RULES_FILE" >/dev/null; then
      segment_allowed=true
    fi
  fi

  if [ "$segment_allowed" = false ]; then
    BLOCKED_SEGMENT="$segment"
    BLOCKED_REASON="command not in allowlist"
    break
  fi
done <<<"$(split_command_segments "$RAW_COMMAND")"

# If all segments passed, allow the command
if [ -z "$BLOCKED_SEGMENT" ]; then
  exit 0
fi

# A governed segment was not in the allowlist — block the entire command
log_blocked Bash "$RAW_COMMAND" "$BLOCKED_REASON: $BLOCKED_SEGMENT" guard_allowed_commands.sh "$SESSION"
cat >&2 <<ERRMSG
BLOCKED: $BLOCKED_REASON.

Command: $BLOCKED_SEGMENT

Why:
  This command is within a shared allowed prefix, but its precise form is not
  approved by the global or project regex rules.

What to do:
  Claude Code: Try a different approach, or ask the user whether this command should be allowed.
  User: Add an anchored POSIX extended regex to
        <git-root>/.agents/hooks/rules/allowed_commands.json.
ERRMSG

exit 2
