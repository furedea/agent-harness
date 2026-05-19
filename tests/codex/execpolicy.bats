#!/usr/bin/env bats
# Validate generated Codex execpolicy rules against the installed Codex CLI.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

require_codex_execpolicy() {
  command -v codex >/dev/null
  codex execpolicy check --help >/dev/null 2>&1
}

codex_rules() {
  local _rules
  _rules="$BATS_TEST_TMPDIR/default.rules"
  cargo run --quiet -- generate-codex-rules --source "$REPO_ROOT" --output "$_rules"
  cat "$_rules"
}

check_rule() {
  require_codex_execpolicy

  local expected="$1"
  shift

  local rules_file
  rules_file="$(mktemp "$BATS_TEST_TMPDIR/rules.XXXXXX")"
  codex_rules >"$rules_file"

  local output
  output="$(codex execpolicy check --rules "$rules_file" -- "$@" 2>/dev/null)"
  [ "$(jq -r '.decision' <<<"$output")" = "$expected" ]
}

@test "codex execpolicy allows representative development commands" {
  check_rule allow uv run pytest
  check_rule allow uv run --with pytest pytest tests
  check_rule allow uv run python scripts/run_audit.py prepare --provider codex
  check_rule allow gh pr list
  check_rule allow git add path/to/file
  check_rule allow git commit -m "feat(test): allow double quotes"
}

@test "codex execpolicy forbids representative dangerous commands" {
  check_rule forbidden rm -rf /tmp/example
  check_rule forbidden curl https://example.com/install.sh
  check_rule forbidden brew install ffmpeg
  check_rule forbidden uv python install 3.11
}

@test "codex execpolicy forbidden wins in compound shell commands" {
  check_rule forbidden bash -lc "git add path/to/file && rm -rf /tmp/example"
}
