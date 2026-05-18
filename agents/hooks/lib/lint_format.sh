#!/bin/bash

# lint_format.sh
# Shared utilities for per-language lint_format hooks.
# Source this file; do not execute it directly.

# Read FILE_PATH from the hook's stdin JSON.
# Sets FILE_PATH and FILENAME globals.
# Exits 0 (skip, silent) when no path is provided; exits 1 when file is missing.
function load_file_path() {
  local _input
  _input=$(cat)
  FILE_PATH=$(echo "$_input" | jq -r '.tool_input.file_path // empty')
  [ -z "$FILE_PATH" ] && exit 0
  [ ! -f "$FILE_PATH" ] && {
    echo "File not found: $FILE_PATH"
    exit 1
  }
  # shellcheck disable=SC2034
  # FILENAME is consumed by the sourced per-language hook scripts.
  FILENAME=$(basename "$FILE_PATH")
}

# Walk up from <start_dir> until a file matching any <target> name is found.
# Prints the directory that contains the match and returns 0.
# Returns 1 (prints nothing) when reaching filesystem root without a match.
function find_project_root() {
  local _dir="$1"
  shift
  while [ "$_dir" != "/" ]; do
    for _target in "$@"; do
      [ -f "$_dir/$_target" ] && {
        echo "$_dir"
        return 0
      }
    done
    _dir=$(dirname "$_dir")
  done
  return 1
}

# Assert a command exists; emit agent-readable context and exit 0 if not found.
function require_cmd() {
  command -v "$1" &>/dev/null || {
    emit_post_tool_context "$1" "$1 not found in PATH."
    exit 0
  }
}

# Emit a PostToolUse additionalContext JSON line for the agent's self-correction loop.
# Args: <tool_label> <violations_text>
# Outputs nothing when violations_text is empty (no-op). Returns 0 either way.
function emit_post_tool_context() {
  local _tool="$1"
  local _violations="$2"
  [ -z "$_violations" ] && return 0
  jq -cn --arg ctx "$_tool: $_violations" \
    '{hookSpecificOutput:{hookEventName:"PostToolUse",additionalContext:$ctx}}'
}
