#!/bin/bash
set -euCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Adapt Codex apply_patch input to the shared Claude harness boundary hook.

Usage:
    $0

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly SHARED_HOOK="$HOME/.claude/hooks/guard_harness_files.sh"

function patch_paths() {
  local _input="$1"

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
    ' |
    sort -u
}

function absolute_path() {
  local _file_path="$1"

  case "$_file_path" in
    /* | ~/*) printf '%s\n' "$_file_path" ;;
    *) printf '%s/%s\n' "$PWD" "$_file_path" ;;
  esac
}

function run_shared_hook() {
  local _input="$1"
  local _file_path="$2"

  local _session
  _session="$(jq -r '.session_id // empty' <<<"$_input")"

  local _absolute_path
  _absolute_path="$(absolute_path "$_file_path")"

  jq -n \
    --arg file_path "$_absolute_path" \
    --arg session "$_session" \
    '{
      tool_name: "apply_patch",
      tool_input: {
        file_path: $file_path
      },
      session_id: $session
    }' |
    "$SHARED_HOOK"
}

function main() {
  if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
  fi

  if [[ ! -x "$SHARED_HOOK" ]]; then
    cat >&2 <<EOF
BLOCKED: harness boundary hook is not executable.

Hook: $SHARED_HOOK
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

  local _path
  while IFS= read -r _path; do
    [[ -z "$_path" ]] && continue
    run_shared_hook "$_input" "$_path"
  done < <(patch_paths "$_input")
}

main "$@"
