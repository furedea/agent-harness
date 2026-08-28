#!/usr/bin/env bats
# Validate generated Codex execpolicy rules against the installed Codex CLI.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CODEX_BIN="${CODEX_BIN:-codex}"
  SOURCE="$REPO_ROOT/tests/fixtures/complete-source"
}

require_codex_execpolicy() {
  if ! command -v "$CODEX_BIN" >/dev/null; then
    if [ "${REQUIRE_CODEX_EXECPOLICY:-0}" = "1" ]; then
      echo "Codex CLI is not available: $CODEX_BIN" >&2
      return 1
    fi

    skip "Codex CLI is not available: $CODEX_BIN"
  fi

  if ! "$CODEX_BIN" execpolicy check --help >/dev/null 2>&1; then
    if [ "${REQUIRE_CODEX_EXECPOLICY:-0}" = "1" ]; then
      echo "Codex CLI does not support execpolicy check: $CODEX_BIN" >&2
      return 1
    fi

    skip "Codex CLI does not support execpolicy check: $CODEX_BIN"
  fi
}

codex_rules() {
  local _rules
  _rules="$BATS_TEST_TMPDIR/default.rules"
  cargo run --quiet -- generate-codex-rules --source "$SOURCE" --output "$_rules"
  cat "$_rules"
}

check_rule() {
  require_codex_execpolicy

  local _expected="$1"
  shift

  local _rules_file
  _rules_file="$(mktemp "$BATS_TEST_TMPDIR/rules.XXXXXX")"
  codex_rules >"$_rules_file"

  local _output
  _output="$(
    "$CODEX_BIN" execpolicy check --rules "$_rules_file" -- "$@" 2>/dev/null
  )"
  [ "$(jq -r '.decision' <<<"$_output")" = "$_expected" ]
}

@test "codex execpolicy maps allow permissions" {
  check_rule allow fixture check
}

@test "codex execpolicy maps ask permissions to prompt" {
  check_rule prompt fixture publish
}

@test "codex execpolicy maps deny permissions to forbidden" {
  check_rule forbidden fixture destroy
}
