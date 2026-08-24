#!/bin/bash
# Shared command prefix and regex policy helpers.

function command_rules_project_file() {
  local _file_name="$1"
  local _start="${AGENT_PROJECT_DIR:-${CLAUDE_PROJECT_DIR:-$PWD}}"
  local _root

  _root=$(git -C "$_start" rev-parse --show-toplevel 2>/dev/null || true)
  [ -n "$_root" ] || return 1
  echo "$_root/.agents/hooks/rules/$_file_name"
}

function command_rules_validate_prefix_file() {
  local _file="$1"

  jq -e '
    type == "object" and
    .version == 1 and
    (.rules | type == "array" and length > 0 and all(.[];
      (.decision == "allow" or .decision == "forbidden") and
      (.pattern | type == "array" and length > 0 and all(.[]; type == "string" and length > 0)) and
      (.examples | type == "array" and length > 0 and all(.[]; type == "string" and length > 0)) and
      (.justification | type == "string" and length > 0)
    ))
  ' "$_file" >/dev/null 2>&1
}

function command_rules_validate_regex_file() {
  local _file="$1"
  local _pattern
  local _status

  jq -e '
    type == "object" and
    .version == 1 and
    (.rules | type == "array" and length > 0 and all(.[];
      (.patterns | type == "array" and length > 0 and all(.[]; type == "string" and length > 0)) and
      (.justification | type == "string" and length > 0)
    ))
  ' "$_file" >/dev/null 2>&1 || return 1

  while IFS= read -r _pattern; do
    _status=0
    printf '' | grep -qE -e "$_pattern" || _status=$?
    [ "$_status" -lt 2 ] || return 1
  done < <(jq -r '.rules[].patterns[]' "$_file")
}

function command_rules_prefix_reason() {
  local _segment="$1"
  local _file="$2"
  local _decision="$3"
  local _rule
  local _prefix

  while IFS= read -r _rule; do
    _prefix=$(echo "$_rule" | jq -r '.pattern | join(" ")')
    if [[ "$_segment" == "$_prefix" || "$_segment" == "$_prefix "* ]]; then
      echo "$_rule" | jq -r '.justification'
      return 0
    fi
  done < <(jq -c --arg decision "$_decision" '.rules[] | select(.decision == $decision)' "$_file")
  return 1
}

function command_rules_regex_reason() {
  local _segment="$1"
  local _file="$2"
  local _rule
  local _pattern

  [ -f "$_file" ] || return 1
  while IFS= read -r _rule; do
    while IFS= read -r _pattern; do
      if echo "$_segment" | grep -qE -e "$_pattern"; then
        echo "$_rule" | jq -r '.justification'
        return 0
      fi
    done < <(echo "$_rule" | jq -r '.patterns[]')
  done < <(jq -c '.rules[]' "$_file")
  return 1
}
