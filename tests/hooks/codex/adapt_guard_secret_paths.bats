#!/usr/bin/env bats
# Tests for codex/hooks/adapt_guard_secret_paths.sh

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  HOOK="$REPO_ROOT/codex/hooks/adapt_guard_secret_paths.sh"
}

install_policy() {
  local _home="$1"

  mkdir -p "$_home/.claude/hooks/rules" "$_home/.claude/hooks/lib"
  cp "$REPO_ROOT/agents/secret_path_policy.json" "$_home/.claude/hooks/rules/"
  cp "$REPO_ROOT/agents/hooks/lib/audit_log.sh" "$_home/.claude/hooks/lib/"
}

@test "prints usage with --help" {
  run "$HOOK" --help
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage:"* ]]
}

@test "command mode blocks reading env files" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_policy "$_tmp"

  local _input
  _input="$(jq -n --arg cwd "$REPO_ROOT" '{cwd:$cwd,tool_input:{command:"cat .env"},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' command"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
  [[ "$output" == *".env"* ]]
}

@test "command mode blocks home credential paths" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_policy "$_tmp"

  local _input
  _input="$(jq -n '{tool_input:{command:"cat ~/.docker/config.json"},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' command"
  [ "$status" -eq 2 ]
  [[ "$output" == *"~/.docker/config.json"* ]]
}

@test "command mode allows non-path words that contain token" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_policy "$_tmp"

  local _input
  _input="$(jq -n '{tool_input:{command:"rg token README.md"},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' command"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "patch mode blocks apply_patch targets matching secret paths" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_policy "$_tmp"

  local _patch
  _patch='*** Begin Patch
*** Update File: .env.local
@@
-OLD=1
+NEW=1
*** End Patch'

  local _input
  _input="$(jq -n --arg cwd "$REPO_ROOT" --arg command "$_patch" \
    '{cwd:$cwd,tool_input:{command:$command},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' patch"
  [ "$status" -eq 2 ]
  [[ "$output" == *".env.local"* ]]
}

@test "patch mode blocks direct edit paths matching secret paths" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  install_policy "$_tmp"

  local _input
  _input="$(jq -n '{tool_input:{file_path:"~/.ssh/config"},session_id:"sess-codex"}')"

  run env HOME="$_tmp" CLAUDE_PROJECT_DIR="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' patch"
  [ "$status" -eq 2 ]
  [[ "$output" == *"~/.ssh/config"* ]]
}

@test "blocks when policy is missing" {
  local _tmp
  _tmp="$(mktemp -d "$BATS_TEST_TMPDIR/codex.XXXXXX")"
  mkdir -p "$_tmp/.claude/hooks/rules"

  local _input
  _input='{"tool_input":{"command":"cat .env"}}'

  run env HOME="$_tmp" bash -c "printf '%s' '$_input' | '$HOOK' command"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
  [[ "$output" == *"secret path policy"* ]]
}
