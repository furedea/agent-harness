#!/bin/bash
# Shared helper: split a compound shell command on unquoted operators.
# Sourced by .claude/hooks/guard_allowed_commands.sh and guard_dangerous_git.sh.
#
# Splits on `|`, `||`, `&&`, `;`, and background `&`, while respecting:
#   - single-quoted strings
#   - double-quoted strings
#   - backslash escapes outside single quotes (so `\&`, `\|`, `\;` stay
#     part of their segment instead of being mistaken for operators)
#
# Usage:
#   source "$HOME/.claude/hooks/lib/shell_parse.sh"
#   split_command_segments "$some_command"   # one segment per line on stdout

function split_command_segments() {
  local _cmd="$1"
  local _in_sq=false
  local _in_dq=false
  local _escaped=false
  local _segment=""
  local _i _c _next _prev

  for ((_i = 0; _i < ${#_cmd}; _i++)); do
    _c="${_cmd:$_i:1}"
    _next="${_cmd:$((_i + 1)):1}"

    # Previous char was a backslash outside single quotes: treat this char
    # as literal (do not split on it even if it is `&` / `|` / `;`).
    if [[ "$_escaped" == true ]]; then
      _segment+="$_c"
      _escaped=false
      continue
    fi

    # Backslash outside single quotes starts an escape.
    if [[ "$_c" == "\\" && "$_in_sq" == false ]]; then
      _segment+="$_c"
      _escaped=true
      continue
    fi

    # Single-quote toggle (only outside double quotes).
    if [[ "$_c" == "'" && "$_in_dq" == false ]]; then
      if [[ "$_in_sq" == true ]]; then _in_sq=false; else _in_sq=true; fi
      _segment+="$_c"
      continue
    fi

    # Double-quote toggle (only outside single quotes).
    if [[ "$_c" == '"' && "$_in_sq" == false ]]; then
      if [[ "$_in_dq" == true ]]; then _in_dq=false; else _in_dq=true; fi
      _segment+="$_c"
      continue
    fi

    # Inside any quote: treat separators as literal.
    if [[ "$_in_sq" == true || "$_in_dq" == true ]]; then
      _segment+="$_c"
      continue
    fi

    # Outside quotes: split on shell operators.
    _prev="${_cmd:$((_i - 1)):1}"
    if [[ "$_c" == "|" && "$_next" == "|" ]]; then
      echo "$_segment"
      _segment=""
      ((_i++))
    elif [[ "$_c" == "&" && "$_next" == "&" ]]; then
      echo "$_segment"
      _segment=""
      ((_i++))
    elif [[ "$_c" == "|" ]]; then
      echo "$_segment"
      _segment=""
    elif [[ "$_c" == ";" ]]; then
      echo "$_segment"
      _segment=""
    elif [[ "$_c" == "&" && "$_prev" != ">" && "$_next" != ">" ]]; then
      echo "$_segment"
      _segment=""
    else
      _segment+="$_c"
    fi
  done
  echo "$_segment"
}
