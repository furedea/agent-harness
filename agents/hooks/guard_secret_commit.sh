#!/bin/bash
# Claude Code PreToolUse hook: block git commit if staged files match sensitive patterns.
# This hook is triggered before Bash tool calls containing "git commit".
# Exit code 0 = allow, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

HOOK_DIR="$(dirname "${BASH_SOURCE[0]}")"
readonly HOOK_DIR
readonly DEFAULT_POLICY_FILE="$HOOK_DIR/rules/secret_commit_policy.json"
readonly POLICY_FILE="${AGENT_SECRET_COMMIT_POLICY:-$DEFAULT_POLICY_FILE}"

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty')

# Only check git commit commands
if ! echo "$COMMAND" | grep -qE '^\s*git\s+commit'; then
  exit 0
fi

function is_valid_policy() {
  if [[ ! -f "$POLICY_FILE" ]] || ! jq -e '
    type == "object" and
    .version == 1 and
    (.rules | type == "array" and length > 0 and all(.[];
      type == "object" and
      (.pattern | type == "string" and length > 0) and
      (.reason | type == "string" and length > 0)
    ))
  ' "$POLICY_FILE" >/dev/null 2>&1; then
    return 1
  fi

  local _pattern
  while IFS= read -r _pattern; do
    local _status=0
    grep -E "$_pattern" </dev/null >/dev/null 2>&1 || _status=$?
    if [[ "$_status" -gt 1 ]]; then
      return 1
    fi
  done < <(jq -r '.rules[].pattern' "$POLICY_FILE")

  return 0
}

function sensitive_path_reason() {
  local _path="$1"
  local _rule
  while IFS= read -r _rule; do
    local _pattern
    _pattern="$(jq -r '.pattern' <<<"$_rule")"
    if grep -qiE "$_pattern" <<<"$_path"; then
      jq -r '.reason' <<<"$_rule"
      return 0
    fi
  done < <(jq -c '.rules[]' "$POLICY_FILE")
  return 1
}

if ! is_valid_policy; then
  log_blocked Bash "$COMMAND" "invalid secret commit policy: $POLICY_FILE" guard_secret_commit.sh "$SESSION"
  echo "BLOCKED: invalid secret commit policy: $POLICY_FILE" >&2
  exit 2
fi

BLOCKED_FILES=()
BLOCKED_REASONS=()

while IFS= read -r -d '' file; do
  if reason="$(sensitive_path_reason "$file")"; then
    BLOCKED_FILES+=("$file")
    BLOCKED_REASONS+=("$reason")
  fi
done < <(git diff --cached --name-only -z 2>/dev/null)

if [ ${#BLOCKED_FILES[@]} -gt 0 ]; then
  log_blocked Bash "$COMMAND" "staged files match sensitive filename patterns: ${BLOCKED_FILES[*]}" guard_secret_commit.sh "$SESSION"
  echo "BLOCKED: Commit rejected — staged files may contain secrets." >&2
  echo "" >&2
  echo "The following files match sensitive filename patterns and must not be committed:" >&2
  for index in "${!BLOCKED_FILES[@]}"; do
    echo "  - ${BLOCKED_FILES[$index]} (${BLOCKED_REASONS[$index]})" >&2
  done
  echo "" >&2
  echo "Action required:" >&2
  echo "  1. Unstage these files: git reset HEAD <file>" >&2
  echo "  2. Ensure secrets are never staged. Use .gitignore or store them in Google Secret Manager." >&2
  echo "  3. If unsure, ask the user for guidance before proceeding." >&2
  exit 2
fi

exit 0
