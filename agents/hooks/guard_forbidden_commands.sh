#!/bin/bash
# Claude Code PreToolUse hook: block forbidden shell command prefixes from generated policy rules.
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
readonly DEFAULT_COMMAND_POLICY_FILE="$HOOK_DIR/rules/command_policy.json"
readonly DEFAULT_RULES_FILE="$HOOK_DIR/rules/forbidden_commands.json"
readonly COMMAND_POLICY_FILE="${AGENT_COMMAND_POLICY:-$DEFAULT_COMMAND_POLICY_FILE}"
readonly RULES_FILE="${AGENT_FORBIDDEN_COMMAND_RULES:-$DEFAULT_RULES_FILE}"

if ! command -v jq >/dev/null 2>&1; then
  cat >&2 <<ERRMSG
BLOCKED: jq is not installed.

Why: This hook requires jq to parse tool input JSON and generated command policy rules.

What to do:
  Claude Code: Ask the user to install jq.
  User: Install jq through the declarative environment.
ERRMSG
  exit 2
fi

if [ ! -f "$COMMAND_POLICY_FILE" ] || ! command_rules_validate_prefix_file "$COMMAND_POLICY_FILE"; then
  cat >&2 <<ERRMSG
BLOCKED: command prefix policy was not found or is invalid.

Rules: $COMMAND_POLICY_FILE

Why:
  This hook blocks destructive shell commands using Nix-generated rules.

What to do:
  Claude Code: Ask the user to run the Nix switch so generated agent files are refreshed.
ERRMSG
  exit 2
fi

if [ ! -f "$RULES_FILE" ] || ! command_rules_validate_regex_file "$RULES_FILE"; then
  cat >&2 <<ERRMSG
BLOCKED: forbidden command regex rules were not found or are invalid.

Rules: $RULES_FILE

Why:
  This hook cannot safely evaluate fine-grained forbidden command forms.
ERRMSG
  exit 2
fi

PROJECT_RULES_FILE=$(command_rules_project_file forbidden_commands.json || true)
if [ -n "$PROJECT_RULES_FILE" ] && [ -f "$PROJECT_RULES_FILE" ] &&
  ! command_rules_validate_regex_file "$PROJECT_RULES_FILE"; then
  echo "BLOCKED: invalid project forbidden command rules: $PROJECT_RULES_FILE" >&2
  exit 2
fi

INPUT=$(cat)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
if ! RAW_COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null); then
  cat >&2 <<ERRMSG
BLOCKED: failed to parse tool input JSON.

Why: The hook received invalid JSON input and cannot validate the command.

What to do:
  Claude Code: Report this error to the user.
ERRMSG
  exit 2
fi

function normalize_segment() {
  echo "$1" | sed -E 's/^[[:space:]]+|[[:space:]]+$//; s/[[:space:]]+(2>&1|2>\/dev\/null|>&2)[[:space:]]*$//'
}

BLOCKED_SEGMENT=""
BLOCKED_REASON=""

while IFS= read -r segment; do
  segment=$(normalize_segment "$segment")
  [ -z "$segment" ] && continue

  if reason=$(command_rules_prefix_reason "$segment" "$COMMAND_POLICY_FILE" forbidden); then
    BLOCKED_SEGMENT="$segment"
    BLOCKED_REASON="$reason"
  elif reason=$(command_rules_regex_reason "$segment" "$RULES_FILE"); then
    BLOCKED_SEGMENT="$segment"
    BLOCKED_REASON="$reason"
  elif [ -n "$PROJECT_RULES_FILE" ] &&
    reason=$(command_rules_regex_reason "$segment" "$PROJECT_RULES_FILE"); then
    BLOCKED_SEGMENT="$segment"
    BLOCKED_REASON="$reason"
  fi

  [ -n "$BLOCKED_SEGMENT" ] && break
done <<<"$(split_command_segments "$RAW_COMMAND")"

if [ -z "$BLOCKED_SEGMENT" ]; then
  exit 0
fi

log_blocked Bash "$RAW_COMMAND" "$BLOCKED_REASON: $BLOCKED_SEGMENT" guard_forbidden_commands.sh "$SESSION"
cat >&2 <<ERRMSG
BLOCKED: forbidden command.

Command: $BLOCKED_SEGMENT

Why:
  $BLOCKED_REASON

What to do:
  Claude Code: Use a non-destructive approach, or ask the user to run this command manually.
ERRMSG

exit 2
