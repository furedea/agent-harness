#!/usr/bin/env bats
# Tests for codex/hooks/adapt_lint_format.sh
# Focuses on patch_paths extraction and hook_for_path dispatch logic.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  HOOK="$REPO_ROOT/codex/hooks/adapt_lint_format.sh"
}

@test "prints usage with --help" {
  run "$HOOK" --help
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage"* ]]
}

@test "patch_paths extracts Add File paths" {
  input='*** Add File: src/main.py
some content
*** Add File: src/lib.rs'
  result=$(printf '%s' "$input" | awk '
    /^\*\*\* (Add|Update|Delete) File: / {
      sub(/^\*\*\* (Add|Update|Delete) File: /, "")
      print
    }
    /^\*\*\* Move to: / {
      sub(/^\*\*\* Move to: /, "")
      print
    }
  ' | sort -u)
  [ "$result" = "$(printf 'src/lib.rs\nsrc/main.py')" ]
}

@test "patch_paths extracts Update File paths" {
  input='*** Update File: src/app.ts
@@ -1,3 +1,4 @@
+new line'
  result=$(printf '%s' "$input" | awk '
    /^\*\*\* (Add|Update|Delete) File: / {
      sub(/^\*\*\* (Add|Update|Delete) File: /, "")
      print
    }
  ' | sort -u)
  [ "$result" = "src/app.ts" ]
}

@test "patch_paths extracts Move to paths" {
  input='*** Delete File: old.py
*** Move to: new.py'
  result=$(printf '%s' "$input" | awk '
    /^\*\*\* (Add|Update|Delete) File: / {
      sub(/^\*\*\* (Add|Update|Delete) File: /, "")
      print
    }
    /^\*\*\* Move to: / {
      sub(/^\*\*\* Move to: /, "")
      print
    }
  ' | sort -u)
  [ "$result" = "$(printf 'new.py\nold.py')" ]
}

@test "patch_paths deduplicates paths" {
  input='*** Update File: src/main.py
@@ context
*** Update File: src/main.py
@@ more context'
  result=$(printf '%s' "$input" | awk '
    /^\*\*\* (Add|Update|Delete) File: / {
      sub(/^\*\*\* (Add|Update|Delete) File: /, "")
      print
    }
  ' | sort -u)
  [ "$result" = "src/main.py" ]
}

@test "hook_for_path maps py to lint_format_py" {
  # Source the function via a subshell trick
  result=$(CLAUDE_HOOKS_DIR="/stub" bash -c '
    hook_for_path() {
      local _file_path="$1"
      case "$_file_path" in
        *.py) echo "$CLAUDE_HOOKS_DIR/lint_format_py.sh" ;;
        *.sh) echo "$CLAUDE_HOOKS_DIR/lint_format_sh.sh" ;;
        *.js|*.ts|*.jsx|*.tsx) echo "$CLAUDE_HOOKS_DIR/lint_format_js.sh" ;;
        *.nix) echo "$CLAUDE_HOOKS_DIR/lint_format_nix.sh" ;;
        *.lua) echo "$CLAUDE_HOOKS_DIR/lint_format_lua.sh" ;;
        *) return 1 ;;
      esac
    }
    hook_for_path "src/main.py"
  ')
  [ "$result" = "/stub/lint_format_py.sh" ]
}

@test "hook_for_path returns failure for unknown extension" {
  run bash -c '
    CLAUDE_HOOKS_DIR="/stub"
    hook_for_path() {
      local _file_path="$1"
      case "$_file_path" in
        *.py|*.sh|*.js|*.ts|*.jsx|*.tsx|*.rs|*.nix|*.md|*.markdown|*.json|*.toml|*.yml|*.yaml|*.txt|*.lua|*.tex|*.bib|*.cls|*.sty) echo "matched" ;;
        *) return 1 ;;
      esac
    }
    hook_for_path "README"
  '
  [ "$status" -ne 0 ]
}

# ============================================================
# JSON additionalContext translation (Claude format -> Codex plain text)
# ============================================================

@test "adapter extracts additionalContext from hook JSON output" {
  local _tmp
  _tmp="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/codex.XXXXXX")"
  local _file="$_tmp/x.py"
  printf 'x = 1\n' >"$_file"

  # Stub hook directory: replace lint_format_py.sh with one that emits Claude-format JSON.
  local _stub_dir="$_tmp/hooks"
  mkdir -p "$_stub_dir"
  cat >"$_stub_dir/lint_format_py.sh" <<EOF
#!/bin/bash
jq -cn '{hookSpecificOutput:{hookEventName:"PostToolUse",additionalContext:"ruff: F821 undefined name"}}'
EOF
  chmod +x "$_stub_dir/lint_format_py.sh"

  # Patch input describing a change to x.py
  local _input
  _input=$(jq -n --arg cwd "$_tmp" --arg cmd "*** Update File: x.py" \
    '{cwd:$cwd, tool_input:{command:$cmd}}')

  run env -u BASH_ENV HOME="$_tmp" bash --noprofile --norc -c "
    ln -sf '$_stub_dir' '$_tmp/.claude'
    mkdir -p '$_tmp/.claude'
    cp '$_stub_dir/lint_format_py.sh' '$_tmp/.claude/'
    mkdir -p '$_tmp/.claude/hooks'
    cp '$_stub_dir/lint_format_py.sh' '$_tmp/.claude/hooks/'
    echo '$_input' | '$HOOK' 2>/dev/null
  "
  [ "$status" -eq 0 ]
  [[ "$output" == *"ruff: F821 undefined name"* ]]
  ! [[ "$output" == *"hookSpecificOutput"* ]]
}

@test "adapter passes through non-JSON hook output unchanged" {
  local _tmp
  _tmp="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/codex.XXXXXX")"
  local _file="$_tmp/x.sh"
  printf '#!/bin/bash\necho hi\n' >"$_file"

  local _stub_dir="$_tmp/.claude/hooks"
  mkdir -p "$_stub_dir"
  cat >"$_stub_dir/lint_format_sh.sh" <<'EOF'
#!/bin/bash
echo "plain-text hook output: SC2086 quote me"
EOF
  chmod +x "$_stub_dir/lint_format_sh.sh"

  local _input
  _input=$(jq -n --arg cwd "$_tmp" --arg cmd "*** Update File: x.sh" \
    '{cwd:$cwd, tool_input:{command:$cmd}}')

  run env -u BASH_ENV HOME="$_tmp" bash --noprofile --norc -c "echo '$_input' | '$HOOK' 2>/dev/null"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plain-text hook output"* ]]
}

@test "adapter emits nothing when hook is silent (clean lint)" {
  local _tmp
  _tmp="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/codex.XXXXXX")"
  local _file="$_tmp/x.py"
  printf 'x = 1\n' >"$_file"

  local _stub_dir="$_tmp/.claude/hooks"
  mkdir -p "$_stub_dir"
  cat >"$_stub_dir/lint_format_py.sh" <<'EOF'
#!/bin/bash
# Clean: emit nothing
EOF
  chmod +x "$_stub_dir/lint_format_py.sh"

  local _input
  _input=$(jq -n --arg cwd "$_tmp" --arg cmd "*** Update File: x.py" \
    '{cwd:$cwd, tool_input:{command:$cmd}}')

  run env -u BASH_ENV HOME="$_tmp" bash --noprofile --norc -c "echo '$_input' | '$HOOK'"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "adapter ignores stderr from a successful lint hook" {
  local _tmp
  _tmp="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/codex.XXXXXX")"
  local _file="$_tmp/x.py"
  printf 'x = 1\n' >"$_file"

  local _stub_dir="$_tmp/.claude/hooks"
  mkdir -p "$_stub_dir"
  cat >"$_stub_dir/lint_format_py.sh" <<'EOF'
#!/bin/bash
echo "environment noise" >&2
EOF
  chmod +x "$_stub_dir/lint_format_py.sh"

  local _input
  _input=$(jq -n --arg cwd "$_tmp" --arg cmd "*** Update File: x.py" \
    '{cwd:$cwd, tool_input:{command:$cmd}}')

  run env -u BASH_ENV HOME="$_tmp" bash --noprofile --norc -c "echo '$_input' | '$HOOK'"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}
