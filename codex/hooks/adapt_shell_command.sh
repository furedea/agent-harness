#!/bin/bash
set -euCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Adapt Codex shell command tool input to shared Claude command-policy hooks.

Usage:
    $0 <HOOK_PATH>

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly HOOK_PATH="${1:-}"

function command_from_input() {
  local _input="$1"

  jq -r '.tool_input.command // .tool_input.cmd // empty' <<<"$_input"
}

function main() {
  if [[ "$HOOK_PATH" == "--help" || "$HOOK_PATH" == "-h" || -z "$HOOK_PATH" ]]; then
    usage
  fi

  if [[ ! -x "$HOOK_PATH" ]]; then
    cat >&2 <<EOF
BLOCKED: command policy hook is not executable.

Hook: $HOOK_PATH
EOF
    exit 2
  fi

  local _input
  _input="$(cat)"

  local _cwd
  _cwd="$(jq -r '.cwd // empty' <<<"$_input")"
  if [[ -n "$_cwd" ]]; then
    cd "$_cwd"
  fi

  local _command
  _command="$(command_from_input "$_input")"

  jq -n --arg command "$_command" \
    '{
      tool_name: "Bash",
      tool_input: {
        command: $command
      }
    }' |
    "$HOOK_PATH"
}

main "$@"
