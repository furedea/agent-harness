#!/bin/bash
set -euCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Adapt Codex hook input to the shared secret-path policy.

Usage:
    $0 <command|patch>

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly MODE="${1:-}"
readonly DEFAULT_POLICY_FILE="$HOME/.claude/hooks/rules/secret_path_policy.json"
readonly POLICY_FILE="${AGENT_SECRET_PATH_POLICY:-$DEFAULT_POLICY_FILE}"
readonly AUDIT_LOG="$HOME/.claude/hooks/lib/audit_log.sh"

if [[ -f "$AUDIT_LOG" ]]; then
  # shellcheck disable=SC1090
  source "$AUDIT_LOG"
else
  function log_blocked() {
    true
  }
fi

function require_policy_file() {
  if ! command -v jq >/dev/null 2>&1; then
    cat >&2 <<EOF
BLOCKED: jq is not installed.

Why: This hook requires jq to parse Codex hook input and secret path policy.
EOF
    exit 2
  fi

  if [[ ! -f "$POLICY_FILE" ]]; then
    cat >&2 <<EOF
BLOCKED: secret path policy was not found.

Policy: $POLICY_FILE

Why: This hook blocks access to sensitive paths using the shared agent policy.
EOF
    exit 2
  fi
}

function normalize_path() {
  local _path="$1"

  _path="${_path%\"}"
  _path="${_path#\"}"
  _path="${_path%\'}"
  _path="${_path#\'}"

  if [[ "$_path" == \$HOME/* ]]; then
    # shellcheck disable=SC2088  # policy patterns use literal "~/" paths
    _path="~/${_path#\$HOME/}"
  fi
  if [[ "$_path" == "$HOME/"* ]]; then
    # shellcheck disable=SC2088  # policy patterns use literal "~/" paths
    _path="~/${_path#"$HOME/"}"
  fi

  printf '%s\n' "$_path"
}

function is_path_candidate() {
  local _path="$1"

  [[ "$_path" == */* ]] && return 0
  [[ "$_path" == .* ]] && return 0
  [[ "$_path" == *.* ]] && return 0
  [[ "$_path" == "id_rsa" || "$_path" == "id_ed25519" ]] && return 0

  return 1
}

function pattern_matches_path() {
  local _pattern="$1"
  local _path
  _path="$(normalize_path "$2")"

  # shellcheck disable=SC2053  # policy values are glob patterns by design
  [[ "$_path" == $_pattern ]] && return 0

  if [[ "$_pattern" == \*\*/* ]]; then
    local _without_globstar="${_pattern#\*\*/}"
    # shellcheck disable=SC2053
    [[ "$_path" == $_without_globstar ]] && return 0
  fi

  return 1
}

function command_candidates() {
  local _command="$1"
  local _clean="$_command"

  _clean="${_clean//$'\n'/ }"
  _clean="${_clean//$'\r'/ }"
  _clean="${_clean//$'\t'/ }"
  _clean="${_clean//\"/ }"
  _clean="${_clean//\'/ }"

  local _operator
  for _operator in ";" "|" "&" "<" ">" "(" ")" "{" "}" ","; do
    _clean="${_clean//$_operator/ }"
  done

  local _token
  while IFS= read -r _token; do
    [[ -z "$_token" ]] && continue
    printf '%s\n' "$_token"
    if [[ "$_token" == *=* ]]; then
      printf '%s\n' "${_token#*=}"
    fi
  done < <(tr ' ' '\n' <<<"$_clean")
}

function patch_paths() {
  local _input="$1"

  jq -r '.tool_input.file_path // .tool_input.path // empty' <<<"$_input"
  jq -r '.tool_input.command // empty' <<<"$_input" |
    awk '
      /^\*\*\* (Add|Update|Delete) File: / {
        sub(/^\*\*\* (Add|Update|Delete) File: /, "")
        print
      }
      /^\*\*\* Move to: / {
        sub(/^\*\*\* Move to: /, "")
        print
      }
    '
}

function blocked_rule_for_path() {
  local _path="$1"
  local _rule
  local _pattern

  [[ -z "$_path" ]] && return 1
  is_path_candidate "$_path" || return 1

  while IFS= read -r _rule; do
    _pattern="$(jq -r '.pattern' <<<"$_rule")"
    if pattern_matches_path "$_pattern" "$_path"; then
      printf '%s\n' "$_rule"
      return 0
    fi
  done < <(jq -c '.rules[]' "$POLICY_FILE")

  return 1
}

function block_path() {
  local _tool="$1"
  local _input_summary="$2"
  local _path="$3"
  local _rule="$4"
  local _session="$5"
  local _pattern
  local _reason

  _pattern="$(jq -r '.pattern' <<<"$_rule")"
  _reason="$(jq -r '.reason' <<<"$_rule")"

  log_blocked "$_tool" "$_input_summary" "$_reason: $_path" adapt_guard_secret_paths.sh "$_session"
  cat >&2 <<EOF
BLOCKED: secret path policy matched.

Path: $_path
Pattern: $_pattern

Why:
  $_reason
EOF
  exit 2
}

function check_command() {
  local _input="$1"
  local _command
  _command="$(jq -r '.tool_input.command // .tool_input.cmd // empty' <<<"$_input")"

  local _session
  _session="$(jq -r '.session_id // empty' <<<"$_input")"

  local _candidate
  while IFS= read -r _candidate; do
    local _rule
    if _rule="$(blocked_rule_for_path "$_candidate")"; then
      block_path "Bash" "$_command" "$_candidate" "$_rule" "$_session"
    fi
  done < <(command_candidates "$_command")
}

function check_patch() {
  local _input="$1"
  local _session
  _session="$(jq -r '.session_id // empty' <<<"$_input")"

  local _path
  while IFS= read -r _path; do
    [[ -z "$_path" ]] && continue
    local _rule
    if _rule="$(blocked_rule_for_path "$_path")"; then
      block_path "apply_patch" "$_path" "$_path" "$_rule" "$_session"
    fi
  done < <(patch_paths "$_input" | sort -u)
}

function main() {
  if [[ "$MODE" == "--help" || "$MODE" == "-h" || -z "$MODE" ]]; then
    usage
  fi

  require_policy_file

  local _input
  _input="$(cat)"

  local _cwd
  _cwd="$(jq -r '.cwd // empty' <<<"$_input")"
  if [[ -n "$_cwd" ]]; then
    cd "$_cwd"
  fi

  case "$MODE" in
  command) check_command "$_input" ;;
  patch) check_patch "$_input" ;;
  *) usage ;;
  esac
}

main "$@"
