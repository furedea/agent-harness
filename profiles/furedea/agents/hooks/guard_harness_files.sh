#!/usr/bin/env bash
# Claude Code PreToolUse hook: block edits to installed harness files.
# The permissions/sandbox layer is the hard boundary; this hook adds an
# explanatory block reason plus audit logging before that boundary is reached.
# Exit code 0 = allow, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

HOOK_DIR="$(dirname "${BASH_SOURCE[0]}")"
readonly HOOK_DIR
readonly DEFAULT_POLICY_FILE="$HOOK_DIR/rules/protected_paths.json"
readonly POLICY_FILE="${AGENT_PROTECTED_PATH_POLICY:-$DEFAULT_POLICY_FILE}"

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // "Edit"')
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty')
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // empty')

[ -z "$FILE_PATH" ] && exit 0

if [ ! -f "$POLICY_FILE" ] || ! jq -e '
  type == "object" and
  .version == 1 and
  (.paths | type == "array" and length > 0 and all(.[]; type == "string" and length > 0))
' "$POLICY_FILE" >/dev/null 2>&1; then
  echo "BLOCKED: invalid protected path policy: $POLICY_FILE" >&2
  exit 2
fi

function expand_home_path() {
  local _path="$1"

  # shellcheck disable=SC2088  # policy paths intentionally use literal "~/"
  if [[ "$_path" == "~/"* ]]; then
    printf '%s/%s\n' "$HOME" "${_path:2}"
    return
  fi
  printf '%s\n' "$_path"
}

function is_protected_path() {
  local _file_path
  local _protected_path

  _file_path="$(expand_home_path "$1")"
  while IFS= read -r _protected_path; do
    if [[ "$_file_path" == "$(expand_home_path "$_protected_path")" ]]; then
      return 0
    fi
  done < <(jq -r '.paths[]' "$POLICY_FILE")
  return 1
}

if is_protected_path "$FILE_PATH"; then
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
fi

exit 0
