#!/usr/bin/env bats
# Validate generated Claude settings used for command_policy migration.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  SETTINGS="$REPO_ROOT/claude/settings.base.json"
}

generated_settings() {
  local _settings
  _settings="$BATS_TEST_TMPDIR/generated_settings.json"
  cargo run --quiet -- generate-claude-settings --source "$REPO_ROOT" --output "$_settings"
  cat "$_settings"
}

@test "generated settings are valid JSON" {
  generated_settings | jq empty
}

@test "generated settings preserve non-permission top-level settings" {
  generated="$(generated_settings)"

  [ "$(jq -r '.model' <<<"$generated")" = "$(jq -r '.model' "$SETTINGS")" ]
  [ "$(jq -r '.language' <<<"$generated")" = "$(jq -r '.language' "$SETTINGS")" ]
  [ "$(jq -r 'has("hooks")' "$SETTINGS")" = "false" ]
  jq -e '.hooks.PreToolUse | length > 0' <<<"$generated" >/dev/null
}

@test "generated settings preserve non-Bash permissions" {
  generated="$(generated_settings)"

  # Generated protection entries are verified separately; this test only checks
  # that source-authored non-Bash permissions survive the generation pass.
  filter='.permissions.allow[], .permissions.deny[]
    | select(startswith("Bash(") | not)
    | select(test("^(Edit|Write)\\(") | not)'

  expected="$(jq -r "$filter" "$SETTINGS" | sort)"
  actual="$(jq -r "$filter" <<<"$generated" | sort)"

  [ "$actual" = "$expected" ]
}

@test "generated settings derive Bash allow and deny from command policy" {
  generated="$(generated_settings)"

  jq -e '.permissions.allow[] | select(. == "Bash(uv run:*)")' <<<"$generated" >/dev/null
  jq -e '.permissions.deny[] | select(. == "Bash(rm:*)")' <<<"$generated" >/dev/null
  jq -e '.permissions.deny[] | select(. == "Bash(brew install:*)")' <<<"$generated" >/dev/null
}

@test "generated settings lock every provider hook source file" {
  generated="$(generated_settings)"

  # Use git ls-files (not find) so ignored runtime artifacts (e.g. audit logs
  # under agents/hooks/docs/logs/) must not enter the lock list.
  expected_home_paths="$(
    {
      cd "$REPO_ROOT" && git ls-files agents/hooks |
        sed 's|^agents/hooks/|~/.claude/hooks/|'
      cd "$REPO_ROOT" && git ls-files codex/hooks |
        sed 's|^codex/hooks/|~/.codex/hooks/|'
      printf '%s\n' \
        '~/.claude/CLAUDE.md' \
        '~/.claude/rules/forbidden_commands.json' \
        '~/.claude/settings.json' \
        '~/.codex/AGENTS.md' \
        '~/.codex/hooks.json' \
        '~/.codex/rules/default.rules'
    } | sort -u
  )"
  expected_dotfiles_paths="$(
    {
      cd "$REPO_ROOT" && git ls-files agents/hooks |
        sed "s|^agents/hooks/|${REPO_ROOT}/agents/hooks/|"
      cd "$REPO_ROOT" && git ls-files codex/hooks |
        sed "s|^codex/hooks/|${REPO_ROOT}/codex/hooks/|"
      printf '%s\n' "${REPO_ROOT}/agents/AGENTS.md"
    } | sort -u
  )"
  expected_permission_paths="$(printf '%s\n%s\n' "$expected_home_paths" "$expected_dotfiles_paths" | sort -u)"
  expected_sandbox_paths="$expected_permission_paths"

  edit_paths="$(
    jq -r --arg source "$REPO_ROOT" '
      .permissions.deny[]
      | select(test("^Edit\\((~|/).*\\)$"))
      | capture("^Edit\\((?<p>.*)\\)$").p
      | select(startswith("~/.claude/") or startswith("~/.codex/") or startswith($source))
    ' <<<"$generated" |
      sort
  )"
  write_paths="$(
    jq -r --arg source "$REPO_ROOT" '
      .permissions.deny[]
      | select(test("^Write\\((~|/).*\\)$"))
      | capture("^Write\\((?<p>.*)\\)$").p
      | select(startswith("~/.claude/") or startswith("~/.codex/") or startswith($source))
    ' <<<"$generated" |
      sort
  )"
  sandbox_paths="$(
    jq -r '.sandbox.filesystem.denyWrite[]' <<<"$generated" | sort
  )"

  [ "$edit_paths" = "$expected_permission_paths" ]
  [ "$write_paths" = "$expected_permission_paths" ]
  [ "$sandbox_paths" = "$expected_sandbox_paths" ]
}

@test "generated sandbox merges base filesystem keys with synthesized denyWrite" {
  generated="$(generated_settings)"

  # The harness synthesizes `sandbox.filesystem.denyWrite` for hook auto-lock
  # but must merge — not overwrite — author-declared `sandbox.filesystem.*`
  # in `settings.base.json`. The test compares against the base file directly
  # so adding/removing allowWrite entries needs no test edit.
  expected="$(jq -S '.sandbox.filesystem // {}' "$SETTINGS")"
  actual="$(jq -S '.sandbox.filesystem | del(.denyWrite)' <<<"$generated")"

  [ "$expected" = "$actual" ]
  jq -e '.sandbox.filesystem.denyWrite | length > 0' <<<"$generated" >/dev/null
}

@test "generated settings exclude agents/skills/ from auto-lock" {
  generated="$(generated_settings)"

  ! jq -e '.permissions.deny[] | select(test("~/\\.claude/skills/"))' <<<"$generated" >/dev/null
  ! jq -e '.sandbox.filesystem.denyWrite[] | select(test("~/\\.claude/skills/"))' <<<"$generated" >/dev/null
}

@test "generated settings keep hook commands resolvable" {
  generated="$(generated_settings)"
  missing=()

  while IFS= read -r cmd; do
    resolved="${cmd/\$HOME\/.claude\//$REPO_ROOT/agents/}"
    script="$(echo "$resolved" | awk '{print $1}')"

    if [ ! -f "$script" ]; then
      resolved="${cmd/\$HOME\/.claude\//$REPO_ROOT/claude/}"
      script="$(echo "$resolved" | awk '{print $1}')"
    fi

    if [ ! -f "$script" ]; then
      missing+=("$cmd -> $script")
    fi
  done < <(jq -r '.. | objects | select(.command?) | .command' <<<"$generated")

  if [ ${#missing[@]} -gt 0 ]; then
    printf 'Missing script:\n' >&2
    printf '  %s\n' "${missing[@]}" >&2
    return 1
  fi
}

@test "generated settings register harness mutation blocker for file edits" {
  generated="$(generated_settings)"

  jq -e '
    .hooks.PreToolUse[]
    | select(.matcher == "Write|Edit|MultiEdit")
    | .hooks[]
    | select(.command == "$HOME/.claude/hooks/guard_harness_files.sh")
  ' <<<"$generated" >/dev/null
}
