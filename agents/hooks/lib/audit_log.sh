#!/bin/bash
# Shared audit-log writer for block-style PreToolUse hooks.
#
# Each block hook (guard_allowed_commands, guard_dangerous_git, ...) sources this
# library and calls `log_blocked` just before its `exit 2`, so the audit
# JSONL records the "Blocked" event alongside the PreToolUse intent that
# audit_tool_call.sh emits earlier in the matcher chain. The shape mirrors
# audit_tool_call.sh rows so a single `jq` filter can read both:
#
#   {"ts":"...","event":"Blocked","status":"blocked","tool":"Bash",
#    "input":"<summary>","reason":"<first line of stderr message>",
#    "hook":"<script name>","session":"<session id>"}
#
# Failures are silent — audit is observability, not enforcement. The caller
# must always reach its `exit 2` regardless of whether logging succeeded.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"
#   log_blocked <tool> <input_summary> <reason> <hook_name> <session_id>

log_blocked() {
  command -v jq >/dev/null 2>&1 || return 0

  local _tool="${1:-}"
  local _input="${2:-}"
  local _reason="${3:-}"
  local _hook="${4:-}"
  local _session="${5:-}"

  local _project_dir="${CLAUDE_PROJECT_DIR:-$PWD}"
  local _log_dir="$_project_dir/docs/logs/audit"
  mkdir -p "$_log_dir" 2>/dev/null || true

  local _log_file
  _log_file="$_log_dir/$(date -u +%Y-%m-%d).jsonl"

  local _ts
  _ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

  jq -cn \
    --arg ts "$_ts" \
    --arg tool "$_tool" \
    --arg input "$_input" \
    --arg reason "$_reason" \
    --arg hook "$_hook" \
    --arg session "$_session" \
    '{ts: $ts, event: "Blocked", status: "blocked", tool: $tool, input: $input, reason: $reason, hook: $hook, session: $session}' \
    >>"$_log_file" 2>/dev/null || true
}
