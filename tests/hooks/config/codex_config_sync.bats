#!/usr/bin/env bats
# Tests for syncing managed Codex config keys while preserving Codex-owned state.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  PYTHON="${PYTHON:-python}"
}

sync_config() {
  cargo run --quiet -- sync-codex-config --source "$1" --target "$2"
}

@test "sync preserves trusted projects and marketplace timestamps" {
  source_file="$BATS_TEST_TMPDIR/source.toml"
  target_file="$BATS_TEST_TMPDIR/target.toml"

  cat >"$source_file" <<'TOML'
model = "gpt-5.5"
sandbox_mode = "read-only"

[features]
hooks = true
TOML

  cat >"$target_file" <<'TOML'
model = "gpt-5.4"
sandbox_mode = "danger-full-access"

[features]
hooks = false

[marketplaces.openai-bundled]
last_updated = "2026-04-27T09:45:32Z"

[projects."/Users/kaito/project"]
trust_level = "trusted"
TOML

  run sync_config "$source_file" "$target_file"
  [ "$status" -eq 0 ]

  "$PYTHON" - "$target_file" <<'PY'
import pathlib
import sys
import tomllib

data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
assert data["model"] == "gpt-5.5"
assert data["sandbox_mode"] == "read-only"
assert data["features"]["hooks"] is True
assert data["projects"]["/Users/kaito/project"]["trust_level"] == "trusted"
assert data["marketplaces"]["openai-bundled"]["last_updated"] == "2026-04-27T09:45:32Z"
PY
}

@test "sync removes managed keys missing from source without removing projects" {
  source_file="$BATS_TEST_TMPDIR/source.toml"
  target_file="$BATS_TEST_TMPDIR/target.toml"

  cat >"$source_file" <<'TOML'
model = "gpt-5.5"
TOML

  cat >"$target_file" <<'TOML'
model = "gpt-5.4"
sandbox_mode = "read-only"

[projects."/Users/kaito/project"]
trust_level = "trusted"
TOML

  run sync_config "$source_file" "$target_file"
  [ "$status" -eq 0 ]

  "$PYTHON" - "$target_file" <<'PY'
import pathlib
import sys
import tomllib

data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
assert data["model"] == "gpt-5.5"
assert "sandbox_mode" not in data
assert data["projects"]["/Users/kaito/project"]["trust_level"] == "trusted"
PY
}

@test "sync creates target config when it does not exist" {
  source_file="$BATS_TEST_TMPDIR/source.toml"
  target_file="$BATS_TEST_TMPDIR/new/target.toml"

  cat >"$source_file" <<'TOML'
model = "gpt-5.5"

[features]
hooks = true
TOML

  run sync_config "$source_file" "$target_file"
  [ "$status" -eq 0 ]
  [ -f "$target_file" ]

  "$PYTHON" - "$target_file" <<'PY'
import pathlib
import sys
import tomllib

data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
assert data["model"] == "gpt-5.5"
assert data["features"]["hooks"] is True
PY
}
