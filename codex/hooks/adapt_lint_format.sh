#!/bin/bash
set -euCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Run existing Claude Code lint/format hooks for files changed by apply_patch.

Usage:
    $0

Options:
    --help, -h: print this
EOF
  exit 1
}

readonly CLAUDE_HOOKS_DIR="$HOME/.claude/hooks"

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

function hook_for_path() {
  local _file_path="$1"

  case "$_file_path" in
    *.py) echo "$CLAUDE_HOOKS_DIR/lint_format_py.sh" ;;
    *.sh) echo "$CLAUDE_HOOKS_DIR/lint_format_sh.sh" ;;
    *.js | *.ts | *.jsx | *.tsx) echo "$CLAUDE_HOOKS_DIR/lint_format_js.sh" ;;
    *.rs) echo "$CLAUDE_HOOKS_DIR/lint_format_rs.sh" ;;
    *.nix) echo "$CLAUDE_HOOKS_DIR/lint_format_nix.sh" ;;
    *.md | *.markdown) echo "$CLAUDE_HOOKS_DIR/lint_format_md.sh" ;;
    *.json | *.toml) echo "$CLAUDE_HOOKS_DIR/lint_format_json_toml.sh" ;;
    *.yml | *.yaml) echo "$CLAUDE_HOOKS_DIR/lint_format_gha.sh" ;;
    *.txt) echo "$CLAUDE_HOOKS_DIR/lint_format_txt.sh" ;;
    *.lua) echo "$CLAUDE_HOOKS_DIR/lint_format_lua.sh" ;;
    *.tex | *.bib | *.cls | *.sty) echo "$CLAUDE_HOOKS_DIR/lint_format_tex.sh" ;;
    *) return 1 ;;
  esac
}

function run_file_hook() {
  local _hook="$1"
  local _file_path="$2"

  if [[ ! -f "$_file_path" ]]; then
    return 0
  fi

  local _output
  _output="$(jq -n --arg file_path "$_file_path" \
    '{
			tool_name: "Edit",
			tool_input: {
				file_path: $file_path
			}
		}' |
    "$_hook" 2>&1)" || true

  [[ -z "$_output" ]] && return 0

  # Translate Claude PostToolUse JSON ({hookSpecificOutput.additionalContext})
  # into plain text for Codex, which expects raw stdout/stderr.
  local _ctx
  if _ctx="$(jq -er '.hookSpecificOutput.additionalContext' <<<"$_output" 2>/dev/null)"; then
    printf '%s\n' "$_ctx"
  else
    printf '%s\n' "$_output"
  fi
}

function main() {
  if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
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

    local _hook
    if ! _hook="$(hook_for_path "$_path")"; then
      continue
    fi

    run_file_hook "$_hook" "$_path"
  done < <(patch_paths "$_input")
}

main "$@"
