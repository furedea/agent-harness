#!/bin/bash
set -euCo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# shellcheck disable=SC1091
source "$PWD/lib/audit_log.sh"

function usage() {
  cat <<EOF >&2
Description:
    Scan Claude Code hook input for sensitive information.

Usage:
    $0 <prompt|read|write>

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly MODE="${1:-}"
readonly PATTERNS_FILE="$PWD/rules/secret_content_patterns.json"

function scan_text_from_input() {
  local _input="$1"

  case "$MODE" in
    prompt)
      jq -r '.prompt // empty' <<<"$_input"
      ;;
    read)
      local _file_path
      _file_path="$(jq -r '.tool_input.file_path // empty' <<<"$_input")"
      if [[ -n "$_file_path" && -f "$_file_path" ]]; then
        head -c 100000 "$_file_path" 2>/dev/null || true
      fi
      ;;
    write)
      local _content
      _content="$(jq -r '.tool_input.content // empty' <<<"$_input")"
      local _new_string
      _new_string="$(jq -r '.tool_input.new_string // empty' <<<"$_input")"
      printf '%s%s' "$_content" "$_new_string"
      ;;
    *)
      usage
      ;;
  esac
}

function audit_block_secret() {
  local _input="$1"
  local _rule_name="$2"
  local _message="$3"

  local _session _tool _summary _file_path
  _session="$(jq -r '.session_id // empty' <<<"$_input")"

  case "$MODE" in
    prompt)
      _tool="UserPromptSubmit"
      # NEVER log the prompt body — it may contain the secret that triggered
      # the rule. Audit-log only the matched rule name.
      _summary="<prompt body elided>"
      ;;
    read)
      _tool="Read"
      _file_path="$(jq -r '.tool_input.file_path // empty' <<<"$_input")"
      _summary="$_file_path"
      ;;
    write)
      # PreToolUse hook input does not always carry tool_name when the matcher
      # is "Write|Edit|MultiEdit"; record file_path as the summary so the
      # operator can still see which file the rule blocked.
      _tool="$(jq -r '.tool_name // "Write"' <<<"$_input")"
      _file_path="$(jq -r '.tool_input.file_path // empty' <<<"$_input")"
      _summary="$_file_path"
      ;;
    *)
      _tool="unknown"
      _summary=""
      ;;
  esac

  log_blocked "$_tool" "$_summary" "$_message ($_rule_name)" guard_secret_content.sh "$_session"
}

function block_output() {
  local _reason="$1"

  case "$MODE" in
    prompt)
      jq -n --arg reason "$_reason. Prompt contains sensitive information." \
        '{
				decision: "block",
				reason: $reason
			}'
      ;;
    read | write)
      jq -n --arg reason "$_reason" \
        '{
				hookSpecificOutput: {
					hookEventName: "PreToolUse",
					permissionDecision: "deny",
					permissionDecisionReason: $reason
				}
			}'
      ;;
  esac
}

function main() {
  if [[ "$MODE" == "--help" || "$MODE" == "-h" || -z "$MODE" ]]; then
    usage
  fi

  local _input
  _input="$(cat)"

  if [[ ! -f "$PATTERNS_FILE" ]]; then
    exit 0
  fi

  local _scan_text
  _scan_text="$(scan_text_from_input "$_input")"

  if [[ -z "$_scan_text" ]]; then
    exit 0
  fi

  local _rule_name
  while IFS= read -r _rule_name; do
    local _pattern
    _pattern="$(jq -r --arg k "$_rule_name" '.[$k].pattern // empty' "$PATTERNS_FILE")"
    local _message
    _message="$(jq -r --arg k "$_rule_name" '.[$k].message // empty' "$PATTERNS_FILE")"
    [[ -z "$_pattern" ]] && continue

    if rg --pcre2 -q -e "$_pattern" - <<<"$_scan_text" 2>/dev/null; then
      audit_block_secret "$_input" "$_rule_name" "$_message"
      block_output "BLOCKED: $_message ($_rule_name)"
      exit 0
    fi
  done < <(jq -r 'keys[]' "$PATTERNS_FILE")
}

main "$@"
