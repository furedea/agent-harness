#!/usr/bin/env bats
# Structure and syntax tests for all lint_format_*.sh hooks.
# These hooks depend on external tools (ruff, shfmt, oxlint, etc.),
# so we test structure, syntax, and source wiring rather than execution.

setup() {
  load test-helper/setup
  LINT_HOOKS=(
    "$HOOK_DIR/lint_format_gha.sh"
    "$HOOK_DIR/lint_format_js.sh"
    "$HOOK_DIR/lint_format_json_toml.sh"
    "$HOOK_DIR/lint_format_lua.sh"
    "$HOOK_DIR/lint_format_md.sh"
    "$HOOK_DIR/lint_format_nix.sh"
    "$HOOK_DIR/lint_format_py.sh"
    "$HOOK_DIR/lint_format_rs.sh"
    "$HOOK_DIR/lint_format_sh.sh"
    "$HOOK_DIR/lint_format_tex.sh"
    "$HOOK_DIR/lint_format_txt.sh"
  )
}

# ============================================================
# File existence and permissions
# ============================================================

@test "all lint_format hooks exist" {
  for hook in "${LINT_HOOKS[@]}"; do
    [ -f "$hook" ] || {
      echo "Missing: $hook"
      return 1
    }
  done
}

@test "all lint_format hooks are executable" {
  for hook in "${LINT_HOOKS[@]}"; do
    [ -x "$hook" ] || {
      echo "Not executable: $hook"
      return 1
    }
  done
}

# ============================================================
# Syntax validation
# ============================================================

@test "all lint_format hooks pass bash syntax check" {
  for hook in "${LINT_HOOKS[@]}"; do
    bash -n "$hook" || {
      echo "Syntax error in: $hook"
      return 1
    }
  done
}

# ============================================================
# Source wiring
# ============================================================

@test "all lint_format hooks source lib/lint_format.sh" {
  for hook in "${LINT_HOOKS[@]}"; do
    grep -q 'source.*lib/lint_format.sh' "$hook" || {
      echo "Missing lib/lint_format.sh source: $hook"
      return 1
    }
  done
}

@test "all lint_format hooks call load_file_path" {
  for hook in "${LINT_HOOKS[@]}"; do
    grep -q 'load_file_path' "$hook" || {
      echo "Missing load_file_path call: $hook"
      return 1
    }
  done
}

@test "all lint_format hooks call require_cmd" {
  for hook in "${LINT_HOOKS[@]}"; do
    grep -q 'require_cmd' "$hook" || {
      echo "Missing require_cmd call: $hook"
      return 1
    }
  done
}

@test "all lint_format hooks have set -e" {
  for hook in "${LINT_HOOKS[@]}"; do
    grep -q 'set -e' "$hook" || {
      echo "Missing set -e: $hook"
      return 1
    }
  done
}

# ============================================================
# Shebang
# ============================================================

@test "all lint_format hooks have bash shebang" {
  for hook in "${LINT_HOOKS[@]}"; do
    head -1 "$hook" | grep -q '#!/bin/bash' || {
      echo "Missing bash shebang: $hook"
      return 1
    }
  done
}

# ============================================================
# Exit with no file_path (via lib/lint_format.sh)
# ============================================================

@test "lint_format_py exits 0 when no file_path in input" {
  run bash "$HOOK_DIR/lint_format_py.sh" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}

@test "lint_format_sh exits 0 when no file_path in input" {
  run bash "$HOOK_DIR/lint_format_sh.sh" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}

@test "lint_format_js exits 0 when no file_path in input" {
  run bash "$HOOK_DIR/lint_format_js.sh" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}

@test "lint_format_json_toml exits 0 when no file_path in input" {
  run bash "$HOOK_DIR/lint_format_json_toml.sh" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}

@test "lint_format_gha exits 0 when no file_path in input" {
  run bash "$HOOK_DIR/lint_format_gha.sh" <<< '{"tool_input":{}}'
  [ "$status" -eq 0 ]
}

# ============================================================
# Each hook names the tool it invokes
# ============================================================

@test "lint_format_py references ruff" {
  grep -q 'ruff' "$HOOK_DIR/lint_format_py.sh"
}

@test "lint_format_sh references shfmt and shellcheck" {
  grep -q 'shfmt' "$HOOK_DIR/lint_format_sh.sh"
  grep -q 'shellcheck' "$HOOK_DIR/lint_format_sh.sh"
}

@test "lint_format_js references oxfmt and oxlint" {
  grep -q 'oxfmt' "$HOOK_DIR/lint_format_js.sh"
  grep -q 'oxlint' "$HOOK_DIR/lint_format_js.sh"
}

@test "lint_format_json_toml references dprint check" {
  grep -q 'dprint check' "$HOOK_DIR/lint_format_json_toml.sh"
}

@test "lint_format_json_toml emits dprint diagnostics through PostToolUse context" {
  grep -q 'emit_post_tool_context "dprint"' "$HOOK_DIR/lint_format_json_toml.sh"
}

@test "lint_format_rs references rustfmt" {
  grep -q 'rustfmt' "$HOOK_DIR/lint_format_rs.sh"
}

@test "lint_format_nix references nixfmt" {
  grep -q 'nixfmt' "$HOOK_DIR/lint_format_nix.sh"
}

@test "lint_format_gha references actionlint" {
  grep -q 'actionlint' "$HOOK_DIR/lint_format_gha.sh"
}

@test "lint_format_tex references tex-fmt or chktex" {
  grep -qE 'tex-fmt|chktex' "$HOOK_DIR/lint_format_tex.sh"
}

@test "lint_format_lua references stylua or selene" {
  grep -qE 'stylua|selene' "$HOOK_DIR/lint_format_lua.sh"
}
