#!/usr/bin/env bats
# Validate the generated Codex permissions config that auto-locks hook files.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  PYTHON="${PYTHON:-python}"
}

fragment_toml() {
  local _fragment
  _fragment="$BATS_TEST_TMPDIR/codex_fragment.toml"
  cargo run --quiet -- generate-codex-config-fragment --source "$REPO_ROOT" --output "$_fragment"
  cat "$_fragment"
}

filesystem_entries() {
  fragment_toml | "$PYTHON" -c '
import json
import sys
import tomllib

data = tomllib.loads(sys.stdin.read())
print(json.dumps(data["permissions"]["guarded"]["filesystem"]))
'
}

@test "generated codex fragment is valid TOML" {
  toml="$(fragment_toml)"
  printf '%s' "$toml" | "$PYTHON" -c 'import sys, tomllib; tomllib.loads(sys.stdin.read())'
}

@test "generated codex fragment exposes guarded filesystem section with glob depth" {
  toml="$(fragment_toml)"
  printf '%s' "$toml" | "$PYTHON" -c '
import sys, tomllib
data = tomllib.loads(sys.stdin.read())
filesystem = data["permissions"]["guarded"]["filesystem"]
assert filesystem["glob_scan_max_depth"] >= 3, filesystem
'
}

@test "codex/config.toml does not select guarded permissions by default" {
  CONFIG="$REPO_ROOT/codex/config.toml" "$PYTHON" -c '
import os, tomllib, pathlib
data = tomllib.loads(pathlib.Path(os.environ["CONFIG"]).read_text())
assert "default_permissions" not in data, data
'
}

@test "merged Codex config removes stale default_permissions and keeps guarded profile available" {
  source_file="$BATS_TEST_TMPDIR/source.toml"
  target_file="$BATS_TEST_TMPDIR/target.toml"

  cargo run --quiet -- generate-codex-config-source --source "$REPO_ROOT" --output "$source_file"
  printf 'default_permissions = "guarded"\n' >"$target_file"

  cargo run --quiet -- sync-codex-config --source "$source_file" --target "$target_file"

  TARGET="$target_file" "$PYTHON" -c '
import os, tomllib, pathlib
data = tomllib.loads(pathlib.Path(os.environ["TARGET"]).read_text())
assert "default_permissions" not in data, data
filesystem = data["permissions"]["guarded"]["filesystem"]
assert filesystem["glob_scan_max_depth"] == 5
assert all(v == "read" for k, v in filesystem.items() if k != "glob_scan_max_depth")
assert "~/.claude/hooks/guard_allowed_commands.sh" in filesystem
assert "~/.codex/hooks/adapt_shell_command.sh" in filesystem
'
}

@test "generated codex fragment locks every file under agents/hooks/ and codex/hooks/" {
  # Use git ls-files (not find) so ignored runtime artifacts (e.g. audit logs
  # under agents/hooks/docs/logs/) must not enter the lock list.
  expected="$(
    {
      cd "$REPO_ROOT" && git ls-files agents/hooks |
        sed 's|^agents/hooks/|~/.claude/hooks/|'
      cd "$REPO_ROOT" && git ls-files codex/hooks |
        sed 's|^codex/hooks/|~/.codex/hooks/|'
      cd "$REPO_ROOT" && git ls-files agents/hooks |
        sed "s|^agents/hooks/|${REPO_ROOT}/agents/hooks/|"
      cd "$REPO_ROOT" && git ls-files codex/hooks |
        sed "s|^codex/hooks/|${REPO_ROOT}/codex/hooks/|"
      printf '%s\n' \
        '~/.claude/CLAUDE.md' \
        '~/.claude/rules/forbidden_commands.json' \
        '~/.claude/settings.json' \
        '~/.codex/AGENTS.md' \
        '~/.codex/hooks.json' \
        '~/.codex/rules/default.rules' \
        "${REPO_ROOT}/agents/AGENTS.md"
    } | sort -u
  )"

  actual="$(filesystem_entries | jq -r 'to_entries[] | select(.key != "glob_scan_max_depth") | .key' | sort)"

  [ "$actual" = "$expected" ]
}

@test "generated codex fragment marks every locked path as read-only" {
  filesystem_entries | jq -e '
    to_entries
    | map(select(.key != "glob_scan_max_depth"))
    | all(.value == "read")
  ' >/dev/null
}

@test "generated codex fragment excludes agents/skills/ from auto-lock" {
  ! filesystem_entries | jq -e 'keys[] | select(test("/skills/"))' >/dev/null
}

@test "generated codex config source includes the guarded profile fragment" {
  source_file="$BATS_TEST_TMPDIR/source.toml"

  cargo run --quiet -- generate-codex-config-source --source "$REPO_ROOT" --output "$source_file"

  "$PYTHON" -c '
import pathlib
import sys
import tomllib

data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
filesystem = data["permissions"]["guarded"]["filesystem"]
assert "~/.claude/hooks/guard_allowed_commands.sh" in filesystem
assert "~/.codex/hooks/adapt_shell_command.sh" in filesystem
' "$source_file"
}
