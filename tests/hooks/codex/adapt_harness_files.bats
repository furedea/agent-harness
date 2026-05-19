#!/usr/bin/env bats
# Tests for codex/hooks/adapt_harness_files.sh

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  HOOK="$REPO_ROOT/codex/hooks/adapt_harness_files.sh"
}

install_shared_hook() {
  local _home="$1"

  mkdir -p "$_home/.claude/hooks/lib"
  cp "$REPO_ROOT/agents/hooks/guard_harness_files.sh" "$_home/.claude/hooks/"
  cp "$REPO_ROOT/agents/hooks/lib/audit_log.sh" "$_home/.claude/hooks/lib/"
  chmod +x "$_home/.claude/hooks/guard_harness_files.sh"
}

@test "prints usage with --help" {
  run "$HOOK" --help
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage:"* ]]
}

@test "blocks Codex apply_patch to harness hook source path" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_shared_hook "$_tmp"

  local _patch
  _patch='*** Begin Patch
*** Update File: agents/hooks/guard_allowed_commands.sh
@@
-#!/bin/bash
+#!/bin/bash
*** End Patch'

  local _input
  _input="$(jq -n --arg cwd "$REPO_ROOT" --arg command "$_patch" \
    '{cwd:$cwd,tool_input:{command:$command},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
  [[ "$output" == *"agents/hooks/guard_allowed_commands.sh"* ]]
}

@test "blocks Codex apply_patch when any patched file is protected" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_shared_hook "$_tmp"

  local _patch
  _patch='*** Begin Patch
*** Update File: src/app.py
@@
-old
+new
*** Update File: codex/hooks/adapt_shell_command.sh
@@
-#!/bin/bash
+#!/bin/bash
*** End Patch'

  local _input
  _input="$(jq -n --arg cwd "$REPO_ROOT" --arg command "$_patch" \
    '{cwd:$cwd,tool_input:{command:$command},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"codex/hooks/adapt_shell_command.sh"* ]]
}

@test "allows Codex apply_patch to normal project files" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_shared_hook "$_tmp"

  local _patch
  _patch='*** Begin Patch
*** Update File: src/app.py
@@
-old
+new
*** End Patch'

  local _input
  _input="$(jq -n --arg cwd "$REPO_ROOT" --arg command "$_patch" \
    '{cwd:$cwd,tool_input:{command:$command},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK'"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "blocks when shared hook is missing" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  mkdir -p "$_tmp/.claude/hooks"

  local _input
  _input='{"tool_input":{"command":"*** Update File: src/app.py"}}'

  run env HOME="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}
