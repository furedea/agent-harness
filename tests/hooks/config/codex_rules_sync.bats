#!/usr/bin/env bats
# Validate the shared command policy and generated Codex rules.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
}

codex_rules() {
  local _rules
  _rules="$BATS_TEST_TMPDIR/default.rules"
  cargo run --quiet -- generate-codex-rules --source "$REPO_ROOT" --output "$_rules"
  cat "$_rules"
}

policy_rules_json() {
  jq '.rules' "$REPO_ROOT/agents/command_policy.json"
}

check_rule() {
  local expected="$1"
  shift

  local rules_file
  rules_file="$(mktemp "$BATS_TEST_TMPDIR/rules.XXXXXX")"
  codex_rules >"$rules_file"

  local output
  output="$(codex execpolicy check --rules "$rules_file" -- "$@" 2>/dev/null)"
  [ "$(jq -r '.decision' <<<"$output")" = "$expected" ]
}

settings_bash_prefixes() {
  local decision="$1"
  local _settings
  _settings="$BATS_TEST_TMPDIR/generated_settings_${decision}.json"

  cargo run --quiet -- generate-claude-settings --source "$REPO_ROOT" --output "$_settings"
  jq -r --arg decision "$decision" '
    .permissions[$decision][] |
    select(startswith("Bash(")) |
    capture("^Bash\\((?<prefix>[^:]+):\\*\\)$").prefix
  ' "$_settings"
}

forbidden_commands() {
  local _forbidden
  _forbidden="$BATS_TEST_TMPDIR/forbidden_commands.json"
  cargo run --quiet -- generate-forbidden-commands --source "$REPO_ROOT" --output "$_forbidden"
  jq -r '.[] | .pattern | join(" ")' "$_forbidden"
}

@test "generated forbidden command hook rules come from the shared policy" {
  forbidden="$(forbidden_commands)"
  [[ "$forbidden" == *curl* ]]
  [[ "$forbidden" == *"brew install"* ]]
}

policy_covers_prefix() {
  local decision="$1"
  local prefix="$2"
  local rules_json="$3"

  jq -e --arg decision "$decision" --arg prefix "$prefix" '
    ($prefix | split(" ")) as $command |
    any(.[]; .decision == $decision and $command[0:(.pattern | length)] == .pattern)
  ' <<<"$rules_json" >/dev/null
}

@test "generated codex rules include representative allow and forbidden rules" {
  rules="$(codex_rules)"
  [[ "$rules" == *'decision = "allow"'* ]]
  [[ "$rules" == *'decision = "forbidden"'* ]]
  [[ "$rules" == *'pattern = ["uv","run"]'* ]]
  [[ "$rules" == *'pattern = ["rm"]'* ]]
  [[ "$rules" == *'pattern = ["brew","install"]'* ]]
}

@test "shared policy covers every Claude Bash allow permission" {
  rules_json="$(policy_rules_json)"
  missing=()

  while IFS= read -r prefix; do
    policy_covers_prefix allow "$prefix" "$rules_json" || missing+=("$prefix")
  done < <(settings_bash_prefixes allow)

  if [ ${#missing[@]} -gt 0 ]; then
    printf 'Missing allow policy for Claude permissions:\n' >&2
    printf '  - %s\n' "${missing[@]}" >&2
    return 1
  fi
}

@test "shared policy covers every Claude Bash deny permission" {
  rules_json="$(policy_rules_json)"
  missing=()

  while IFS= read -r prefix; do
    policy_covers_prefix forbidden "$prefix" "$rules_json" || missing+=("$prefix")
  done < <(settings_bash_prefixes deny)

  if [ ${#missing[@]} -gt 0 ]; then
    printf 'Missing forbidden policy for Claude permissions:\n' >&2
    printf '  - %s\n' "${missing[@]}" >&2
    return 1
  fi
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
