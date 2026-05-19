#!/usr/bin/env bats
# Tests for .claude/hooks/guard_secret_content.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/guard_secret_content.sh"
  PATTERNS_FILE="$HOOK_DIR/rules/secret_content_patterns.json"

  # Create a temp directory for test files
  TEST_TMPDIR="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/secret.XXXXXX")"

  # Back up patterns file byte-for-byte so trailing newlines survive restore
  PATTERNS_BACKUP=""
  if [ -f "$PATTERNS_FILE" ]; then
    PATTERNS_BACKUP="$TEST_TMPDIR/patterns.bak"
    cp "$PATTERNS_FILE" "$PATTERNS_BACKUP"
  fi
}

teardown() {
  if [ -n "$PATTERNS_BACKUP" ] && [ -f "$PATTERNS_BACKUP" ]; then
    cp "$PATTERNS_BACKUP" "$PATTERNS_FILE"
  fi
  [ -d "${TEST_TMPDIR:-}" ] && rm -rf "$TEST_TMPDIR"
}

# Helper: check that the hook produced a block/deny JSON decision.
# The script outputs JSON with "decision":"block" (prompt) or
# "permissionDecision":"deny" (read/write) when it detects a secret.
assert_blocked() {
  [[ "$output" == *'"decision"'* ]] || [[ "$output" == *'"permissionDecision"'* ]]
}

# Helper: check that the hook did NOT produce a block/deny JSON decision.
assert_allowed() {
  [[ "$output" != *'"decision"'* ]] && [[ "$output" != *'"permissionDecision"'* ]]
}

# ============================================================
# Prompt mode
# ============================================================

@test "prompt: blocks AWS access key" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'my key is AKIAIOSFODNN7EXAMPLE')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks GitHub token" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks private key header" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input '-----BEGIN RSA PRIVATE KEY-----')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks OpenAI API key" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'key is sk-abcdefghijklmnopqrstuv')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks bearer token" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks database connection string" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'postgres://user:pass@host:5432/db')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: blocks generic secret assignment" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'api_key = "abcdefghijklmnop"')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "prompt: allows safe text" {
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'please fix the bug in main.py')"
  [ "$status" -eq 0 ]
  assert_allowed
  [ -z "$output" ]
}

@test "prompt: allows empty prompt" {
  run bash "$HOOK" prompt <<<'{"prompt":""}'
  [ "$status" -eq 0 ]
  assert_allowed
}

# ============================================================
# Read mode
# ============================================================

@test "read: blocks file containing AWS key" {
  printf 'config = AKIAIOSFODNN7EXAMPLE\n' >"$TEST_TMPDIR/secrets.txt"
  run bash "$HOOK" read <<<"$(make_read_input "$TEST_TMPDIR/secrets.txt")"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "read: blocks file containing connection string" {
  printf 'DB_URL=mysql://root:pass@localhost/mydb\n' >"$TEST_TMPDIR/config.txt"
  run bash "$HOOK" read <<<"$(make_read_input "$TEST_TMPDIR/config.txt")"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "read: allows clean file" {
  printf 'hello world\n' >"$TEST_TMPDIR/clean.txt"
  run bash "$HOOK" read <<<"$(make_read_input "$TEST_TMPDIR/clean.txt")"
  [ "$status" -eq 0 ]
  assert_allowed
}

@test "read: allows nonexistent file" {
  run bash "$HOOK" read <<<"$(make_read_input "$TEST_TMPDIR/nonexistent.txt")"
  [ "$status" -eq 0 ]
  assert_allowed
}

# ============================================================
# Write mode
# ============================================================

@test "write: blocks content containing private key" {
  run bash "$HOOK" write <<<"$(make_write_input '-----BEGIN PRIVATE KEY-----' '')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "write: blocks content containing Anthropic API key" {
  run bash "$HOOK" write <<<"$(make_write_input 'ANTHROPIC_KEY=sk-ant-abcdefghijklmnopqrstuvwxyz' '')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "write: blocks new_string containing secret" {
  run bash "$HOOK" write <<<"$(make_write_input '' 'api_key = "supersecretvalue123"')"
  [ "$status" -eq 0 ]
  assert_blocked
}

@test "write: allows safe content" {
  run bash "$HOOK" write <<<"$(make_write_input 'def hello(): pass' '')"
  [ "$status" -eq 0 ]
  assert_allowed
}

@test "write: allows empty content and new_string" {
  run bash "$HOOK" write <<<"$(make_write_input '' '')"
  [ "$status" -eq 0 ]
  assert_allowed
}

# ============================================================
# Edge cases
# ============================================================

@test "exits with usage when no mode argument" {
  run bash "$HOOK"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Usage"* ]]
}

@test "exits with usage for --help" {
  run bash "$HOOK" --help
  [ "$status" -eq 1 ]
  [[ "$output" == *"Usage"* ]]
}

@test "skips gracefully when patterns file is missing" {
  local backup="$PATTERNS_FILE.bak"
  mv "$PATTERNS_FILE" "$backup"
  run bash "$HOOK" prompt <<<"$(make_prompt_input 'AKIAIOSFODNN7EXAMPLE')"
  mv "$backup" "$PATTERNS_FILE"
  [ "$status" -eq 0 ]
  assert_allowed
}
