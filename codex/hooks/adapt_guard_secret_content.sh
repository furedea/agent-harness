#!/bin/bash
set -euCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Adapt Codex hook input to the shared Claude sensitive-info scanner.

Usage:
    $0 <prompt|apply-patch>

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly MODE="${1:-}"
readonly SHARED_SCANNER="$HOME/.claude/hooks/guard_secret_content.sh"

function added_patch_text() {
  local _input="$1"

  jq -r '.tool_input.command // empty' <<<"$_input" |
    awk '
			/^\+\+\+/ { next }
			/^\+/ { print substr($0, 2) }
		'
}

function main() {
  if [[ "$MODE" == "--help" || "$MODE" == "-h" || -z "$MODE" ]]; then
    usage
  fi

  local _input
  _input="$(cat)"

  case "$MODE" in
    prompt)
      printf '%s' "$_input" | "$SHARED_SCANNER" prompt
      ;;
    apply-patch)
      local _content
      _content="$(added_patch_text "$_input")"
      jq -n --arg content "$_content" \
        '{
				tool_name: "Edit",
				tool_input: {
					content: $content
				}
			}' |
        "$SHARED_SCANNER" write
      ;;
    *)
      usage
      ;;
  esac
}

main "$@"
