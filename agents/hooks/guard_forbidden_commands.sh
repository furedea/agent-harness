#!/bin/bash
# Claude Code PreToolUse hook: block forbidden shell command prefixes from generated policy rules.
# Exit code 0 = allow/pass-through, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/shell_parse.sh"
# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

readonly DEFAULT_RULES_FILE="$HOME/.claude/hooks/rules/forbidden_commands.json"
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

if [ ! -f "$RULES_FILE" ]; then
  cat >&2 <<ERRMSG
BLOCKED: forbidden command rules were not found.

Rules: $RULES_FILE

Why:
  This hook blocks destructive shell commands using Nix-generated rules.

What to do:
  Claude Code: Ask the user to run the Nix switch so generated agent files are refreshed.
ERRMSG
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

function pattern_prefix() {
  local _rule="$1"
  echo "$_rule" | jq -r '.pattern | join(" ")'
}

function rule_reason() {
  local _rule="$1"
  echo "$_rule" | jq -r '.justification'
}

function segment_matches_prefix() {
  local _segment="$1"
  local _prefix="$2"

  [[ "$_segment" == "$_prefix" || "$_segment" == "$_prefix "* ]]
}

BLOCKED_SEGMENT=""
BLOCKED_REASON=""

while IFS= read -r segment; do
  segment=$(normalize_segment "$segment")
  [ -z "$segment" ] && continue

  while IFS= read -r rule; do
    prefix=$(pattern_prefix "$rule")
    if segment_matches_prefix "$segment" "$prefix"; then
      BLOCKED_SEGMENT="$segment"
      BLOCKED_REASON=$(rule_reason "$rule")
      break
    fi
  done < <(jq -c '.[]' "$RULES_FILE")

  [ -n "$BLOCKED_SEGMENT" ] && break
done <<<"$(split_command_segments "$RAW_COMMAND")"

if [ -z "$BLOCKED_SEGMENT" ]; then
  exit 0
fi

log_blocked Bash "$RAW_COMMAND" "$BLOCKED_REASON: $BLOCKED_SEGMENT" guard_forbidden_commands.sh "$SESSION"
cat >&2 <<ERRMSG
BLOCKED: forbidden command prefix.

Command: $BLOCKED_SEGMENT

Why:
  $BLOCKED_REASON

What to do:
  Claude Code: Use a non-destructive approach, or ask the user to run this command manually.
ERRMSG

exit 2
