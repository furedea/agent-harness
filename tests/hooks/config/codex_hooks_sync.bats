#!/usr/bin/env bats
# Validate generated Codex hook commands.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
}

generated_hooks() {
  local _hooks
  _hooks="$BATS_TEST_TMPDIR/codex_hooks.json"
  cargo run --quiet -- generate-codex-hooks --source "$REPO_ROOT" --output "$_hooks"
  cat "$_hooks"
}

@test "generated hooks are valid JSON" {
  generated_hooks | jq empty
}

@test "all hook commands reference existing scripts" {
  local missing=()

  while IFS= read -r cmd; do
    local resolved
    resolved=$(echo "$cmd" |
      sed "s|\\\$HOME/.claude/hooks/|$REPO_ROOT/agents/hooks/|" |
      sed "s|\\\$HOME/.codex/hooks/|$REPO_ROOT/codex/hooks/|")

    local script
    script=$(echo "$resolved" | awk '{print $1}')

    if [ ! -f "$script" ]; then
      missing+=("$cmd -> $script")
    fi
  done < <(generated_hooks | jq -r '.. | objects | select(.command?) | .command')

  if [ ${#missing[@]} -gt 0 ]; then
    printf 'Missing script:\n' >&2
    printf '  %s\n' "${missing[@]}" >&2
    return 1
  fi
}

@test "no duplicate hooks within the same event group" {
  local dupes
  dupes=$(generated_hooks | jq -r '
    .hooks | to_entries[] |
    .value[] |
    [.hooks[]?.command] |
    group_by(.) |
    map(select(length > 1)) |
    .[][0]
  ' 2>/dev/null || true)

  [ -z "$dupes" ]
}

@test "generated hooks include shell command adapter" {
  generated_hooks | jq -e '
    .hooks.PreToolUse[]
    | select(.matcher | test("Bash"))
    | .hooks[]
    | select(.command | startswith("$HOME/.codex/hooks/adapt_shell_command.sh "))
  ' >/dev/null
}
