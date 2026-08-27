#!/usr/bin/env bats
# Tests for .claude/hooks/notify_macos_done.sh and notify_macos_await.sh
#
# These hooks depend on macOS-specific commands (osascript, afplay).

setup() {
  load test-helper/setup
  DONE_HOOK="$HOOK_DIR/notify_macos_done.sh"
  AWAIT_HOOK="$HOOK_DIR/notify_macos_await.sh"
  FAKE_BIN="$BATS_TEST_TMPDIR/bin"
  COMMAND_LOG="$BATS_TEST_TMPDIR/commands.log"
  mkdir -p "$FAKE_BIN"
}

# ============================================================
# File structure
# ============================================================

@test "notify_macos_done.sh exists and is executable" {
  [ -f "$DONE_HOOK" ]
  [ -x "$DONE_HOOK" ]
}

@test "notify_macos_await.sh exists and is executable" {
  [ -f "$AWAIT_HOOK" ]
  [ -x "$AWAIT_HOOK" ]
}

@test "notify_macos_done.sh has set -euCo pipefail" {
  grep -q 'set -euCo pipefail' "$DONE_HOOK"
}

@test "notify_macos_await.sh has set -euCo pipefail" {
  grep -q 'set -euCo pipefail' "$AWAIT_HOOK"
}

@test "notify_macos_done.sh uses osascript" {
  grep -q 'osascript' "$DONE_HOOK"
}

@test "notify_macos_await.sh uses osascript" {
  grep -q 'osascript' "$AWAIT_HOOK"
}

@test "notify_macos_done.sh uses afplay" {
  grep -q 'afplay' "$DONE_HOOK"
}

@test "notify_macos_await.sh uses afplay" {
  grep -q 'afplay' "$AWAIT_HOOK"
}

# ============================================================
# Syntax validation
# ============================================================

@test "notify_macos_done.sh passes bash syntax check" {
  run bash -n "$DONE_HOOK"
  [ "$status" -eq 0 ]
}

@test "notify_macos_await.sh passes bash syntax check" {
  run bash -n "$AWAIT_HOOK"
  [ "$status" -eq 0 ]
}

@test "notification hooks skip macOS commands outside Darwin" {
  make_fake_command uname 'echo Linux'
  make_fake_command osascript 'echo osascript >>"${COMMAND_LOG:?}"'
  make_fake_command afplay 'echo afplay >>"${COMMAND_LOG:?}"'

  local _hook
  for _hook in "$DONE_HOOK" "$AWAIT_HOOK"; do
    PATH="$FAKE_BIN:$PATH" COMMAND_LOG="$COMMAND_LOG" run bash "$_hook"
    [ "$status" -eq 0 ]
  done

  [ ! -e "$COMMAND_LOG" ]
}

@test "notification hooks skip when a macOS command is unavailable" {
  make_fake_command uname 'echo Darwin'
  make_fake_command afplay 'echo afplay >>"${COMMAND_LOG:?}"'

  local _hook
  for _hook in "$DONE_HOOK" "$AWAIT_HOOK"; do
    PATH="$FAKE_BIN" COMMAND_LOG="$COMMAND_LOG" run /bin/bash "$_hook"
    [ "$status" -eq 0 ]
  done

  [ ! -e "$COMMAND_LOG" ]
}

# ============================================================
# Sound file references
# ============================================================

@test "notify_macos_done.sh references a system sound file" {
  grep -qE '/System/Library/Sounds/.*\.aiff' "$DONE_HOOK"
}

@test "notify_macos_await.sh references a system sound file" {
  grep -qE '/System/Library/Sounds/.*\.aiff' "$AWAIT_HOOK"
}

@test "done and await use different notification sounds" {
  local done_sound await_sound
  done_sound=$(grep -oE '/System/Library/Sounds/[^"]+' "$DONE_HOOK")
  await_sound=$(grep -oE '/System/Library/Sounds/[^"]+' "$AWAIT_HOOK")
  [ "$done_sound" != "$await_sound" ]
}

@test "done and await use different notification messages" {
  local done_msg await_msg
  done_msg=$(grep -o 'display notification "[^"]*"' "$DONE_HOOK")
  await_msg=$(grep -o 'display notification "[^"]*"' "$AWAIT_HOOK")
  [ "$done_msg" != "$await_msg" ]
}

function make_fake_command() {
  local _name="$1"
  local _body="$2"

  printf '#!/bin/bash\n%s\n' "$_body" >"$FAKE_BIN/$_name"
  chmod +x "$FAKE_BIN/$_name"
}
