#!/bin/bash

# run_related_tests.sh
# Stop hook: differential test gate. Blocks completion if relevant tests fail.
# Always exits 0 (block signal is JSON, not status).
#
# Test selection combines three sources:
#   1. Project-specific extension rules at
#      <repo>/agents/hooks/rules/related_test_extensions.json
#      (optional). A JSON object whose keys are bash-glob patterns matched
#      against changed paths; values are lists of test files to run. Use
#      this to express fan-out (library -> consumers) and cross-language
#      mappings that the basename heuristic cannot infer.
#   2. Global default rules at agents/hooks/rules/related_test_defaults.json.
#      These define default source extensions, test directories, and basename
#      patterns such as .py -> test_<stem>.py.
#   3. Per-language basename heuristic driven by the global default rules.
#
# When the combined set is non-empty, only those targets run. When the set is
# empty but a relevant language changed, Bats and pytest can fall back to their
# full suites. Rust runs cargo test <stem> for ordinary src/foo.rs files and
# cargo test --test <stem> for matching integration test targets.

set -eo pipefail

INPUT=$(cat)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Loop guard: if Stop hook already fired in this turn, do not re-trigger.
STOP_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // false')
[ "$STOP_ACTIVE" = "true" ] && exit 0

# Must be inside a git repo to detect changes; otherwise skip.
GIT_ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$GIT_ROOT"

CHANGED=$(
  git diff --name-only HEAD 2>/dev/null
  git ls-files --others --exclude-standard 2>/dev/null
)
[ -z "$CHANGED" ] && exit 0

emit_block() {
  local _failures="$1"
  jq -cn --arg reason "Differential tests failed before completion."$'\n\n'"$_failures" \
    '{decision:"block", reason:$reason}'
}

# --- 1. Project extension rules: dispatch JSON-mapped tests by runner. ---
declare -a PROJECT_BATS=()
declare -a PROJECT_PY=()
RULES_FILE="$GIT_ROOT/agents/hooks/rules/related_test_extensions.json"
LANGUAGE_RULES_FILE="$GIT_ROOT/agents/hooks/rules/related_test_defaults.json"
if [ ! -f "$LANGUAGE_RULES_FILE" ]; then
  LANGUAGE_RULES_FILE="$SCRIPT_DIR/rules/related_test_defaults.json"
fi
if [ -f "$RULES_FILE" ] && jq empty "$RULES_FILE" 2>/dev/null; then
  mapfile -t PATTERNS < <(jq -r 'keys[]' "$RULES_FILE")
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    for pattern in "${PATTERNS[@]}"; do
      # shellcheck disable=SC2053  # intentional glob match
      if [[ "$f" == $pattern ]]; then
        while IFS= read -r t; do
          case "$t" in
            *.bats) PROJECT_BATS+=("$t") ;;
            *.py) PROJECT_PY+=("$t") ;;
          esac
        done < <(jq -r --arg k "$pattern" '.[$k][]' "$RULES_FILE")
      fi
    done
  done <<<"$CHANGED"
fi

language_rule() {
  local _language="$1"
  local _query="$2"

  # shellcheck disable=SC2016  # jq programs intentionally reference $language
  if [ -f "$LANGUAGE_RULES_FILE" ] && jq empty "$LANGUAGE_RULES_FILE" 2>/dev/null; then
    jq -r --arg language "$_language" "$_query" "$LANGUAGE_RULES_FILE"
  fi
}

has_changed_extension() {
  local _language="$1"

  while IFS= read -r extension; do
    [ -z "$extension" ] && continue
    echo "$CHANGED" | grep -qF "$extension" && return 0
  done < <(language_rule "$_language" ".[\$language].source_extensions[]?")
  return 1
}

has_project_marker() {
  local _language="$1"
  local _has_markers=false

  while IFS= read -r marker; do
    [ -z "$marker" ] && continue
    _has_markers=true
    [ -f "$marker" ] && return 0
  done < <(language_rule "$_language" ".[\$language].project_markers[]?")

  [ "$_has_markers" = false ]
}

is_self_test_file() {
  local _language="$1"
  local _file="$2"
  local _name="${_file##*/}"

  while IFS= read -r extension; do
    [ -z "$extension" ] && continue
    [[ "$_file" == *"$extension" ]] && return 0
  done < <(language_rule "$_language" ".[\$language].self_test_extensions[]?")

  while IFS= read -r glob; do
    [ -z "$glob" ] && continue
    # shellcheck disable=SC2053  # intentional glob match
    [[ "$_name" == $glob ]] && return 0
  done < <(language_rule "$_language" ".[\$language].self_test_globs[]?")

  return 1
}

find_language_tests() {
  local _language="$1"
  local _stem="$2"
  local _find_args=()
  local _pattern
  local _first=true

  while IFS= read -r test_dir; do
    [ -d "$test_dir" ] || continue

    _find_args=()
    _first=true
    while IFS= read -r template; do
      [ -z "$template" ] && continue
      _pattern="${template//\{stem\}/$_stem}"
      if [ "$_first" = true ]; then
        _first=false
      else
        _find_args+=(-o)
      fi
      _find_args+=(-name "$_pattern")
    done < <(language_rule "$_language" ".[\$language].test_patterns[]?")

    [ ${#_find_args[@]} -gt 0 ] || continue

    case "$_language" in
      python)
        find "$test_dir" -type f \( "${_find_args[@]}" \) -not -path './.venv/*' -not -path './node_modules/*' 2>/dev/null
        ;;
      *)
        find "$test_dir" -type f \( "${_find_args[@]}" \) 2>/dev/null
        ;;
    esac
  done < <(language_rule "$_language" ".[\$language].test_dirs[]?")
}

FAILURES=""
# Runner timeouts are opportunistic: GNU `timeout` (Linux) or `gtimeout`
# (macOS coreutils) enables the cap; if neither exists, the gate keeps the
# previous direct-run behavior. Override with RUN_RELATED_TESTS_TIMEOUT_SECONDS.
TIMEOUT_SECONDS="${RUN_RELATED_TESTS_TIMEOUT_SECONDS:-120}"

timeout_bin() {
  if command -v timeout >/dev/null 2>&1; then
    echo "timeout"
    return 0
  fi
  if command -v gtimeout >/dev/null 2>&1; then
    echo "gtimeout"
    return 0
  fi
  return 1
}

run_with_timeout() {
  local _label="$1"
  shift

  local _timeout_bin
  if ! _timeout_bin=$(timeout_bin); then
    "$@"
    return $?
  fi

  "$_timeout_bin" "$TIMEOUT_SECONDS" "$@"
  local _status=$?
  if [ "$_status" -eq 124 ]; then
    printf '%s timed out after %ss\n' "$_label" "$TIMEOUT_SECONDS"
  fi
  return "$_status"
}

# --- 2. Bats: project rules + basename heuristic; full-suite fallback. ---
need_bats=0
has_changed_extension bats && need_bats=1
[ ${#PROJECT_BATS[@]} -gt 0 ] && need_bats=1
if [ $need_bats -eq 1 ] && [ -d tests ] && command -v bats >/dev/null 2>&1; then
  declare -a BATS_TARGETS=()
  [ ${#PROJECT_BATS[@]} -gt 0 ] && BATS_TARGETS+=("${PROJECT_BATS[@]}")
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    if is_self_test_file bats "$f" && [ -f "$f" ]; then
      BATS_TARGETS+=("$f")
      continue
    fi
    [[ "$f" == *.sh ]] || continue
    stem="${f##*/}"
    stem="${stem%.*}"
    while IFS= read -r match; do
      BATS_TARGETS+=("$match")
    done < <(find_language_tests bats "$stem")
  done <<<"$CHANGED"

  if [ ${#BATS_TARGETS[@]} -gt 0 ]; then
    mapfile -t BATS_TARGETS < <(printf '%s\n' "${BATS_TARGETS[@]}" | sort -u)
    declare -a BATS_EXIST=()
    for t in "${BATS_TARGETS[@]}"; do
      [ -f "$t" ] && BATS_EXIST+=("$t")
    done
    if [ ${#BATS_EXIST[@]} -gt 0 ]; then
      BATS_OUT=""
      if ! BATS_OUT=$(run_with_timeout "bats" bats "${BATS_EXIST[@]}" 2>&1); then
        FAILURES+="bats:"$'\n'"$BATS_OUT"$'\n\n'
      fi
    fi
  elif has_changed_extension bats; then
    BATS_OUT=""
    if ! BATS_OUT=$(run_with_timeout "bats" bats tests/ --recursive 2>&1); then
      FAILURES+="bats:"$'\n'"$BATS_OUT"$'\n\n'
    fi
  fi
fi

# --- 3. Pytest: project rules + basename heuristic; full pytest fallback. ---
need_pytest=0
has_changed_extension python && need_pytest=1
[ ${#PROJECT_PY[@]} -gt 0 ] && need_pytest=1
if [ $need_pytest -eq 1 ] && has_project_marker python && command -v uv >/dev/null 2>&1; then
  declare -a PY_TARGETS=()
  [ ${#PROJECT_PY[@]} -gt 0 ] && PY_TARGETS+=("${PROJECT_PY[@]}")
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    [[ "$f" == *.py ]] || continue
    if is_self_test_file python "$f" && [ -f "$f" ]; then
      PY_TARGETS+=("$f")
      continue
    fi
    stem="${f##*/}"
    stem="${stem%.py}"
    while IFS= read -r match; do
      PY_TARGETS+=("$match")
    done < <(find_language_tests python "$stem")
  done <<<"$CHANGED"

  if [ ${#PY_TARGETS[@]} -gt 0 ]; then
    mapfile -t PY_TARGETS < <(printf '%s\n' "${PY_TARGETS[@]}" | sort -u)
    declare -a PY_EXIST=()
    for t in "${PY_TARGETS[@]}"; do
      [ -f "$t" ] && PY_EXIST+=("$t")
    done
    if [ ${#PY_EXIST[@]} -gt 0 ]; then
      PYTEST_OUT=""
      if ! PYTEST_OUT=$(run_with_timeout "pytest" uv run --frozen pytest --no-header -q "${PY_EXIST[@]}" 2>&1); then
        FAILURES+="pytest:"$'\n'"$PYTEST_OUT"$'\n\n'
      fi
    fi
  elif has_changed_extension python; then
    PYTEST_OUT=""
    if ! PYTEST_OUT=$(run_with_timeout "pytest" uv run --frozen pytest --no-header -q 2>&1); then
      FAILURES+="pytest:"$'\n'"$PYTEST_OUT"$'\n\n'
    fi
  fi
fi

# --- 4. Rust: unit-name filter plus integration-test targets. `cargo test foo`
# is a substring filter over test names/module paths, so skip generic stems
# like lib/main/mod and use explicit mappings for wider project fan-out.
if has_changed_extension rust && has_project_marker rust && command -v cargo >/dev/null 2>&1; then
  declare -a CARGO_UNIT_FILTERS=()
  declare -a CARGO_TEST_TARGETS=()
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    [[ "$f" == *.rs ]] || continue

    stem="${f##*/}"
    stem="${stem%.rs}"
    case "$f" in
      tests/*.rs)
        CARGO_TEST_TARGETS+=("$stem")
        ;;
      src/*.rs)
        skip_filter=false
        while IFS= read -r skipped_stem; do
          [ -z "$skipped_stem" ] && continue
          if [ "$stem" = "$skipped_stem" ]; then
            skip_filter=true
            break
          fi
        done < <(language_rule rust ".[\$language].skip_unit_filter_stems[]?")
        [ "$skip_filter" = false ] && CARGO_UNIT_FILTERS+=("$stem")

        if [ -f "tests/${stem}.rs" ]; then
          CARGO_TEST_TARGETS+=("$stem")
        fi
        ;;
    esac
  done <<<"$CHANGED"

  if [ ${#CARGO_UNIT_FILTERS[@]} -gt 0 ]; then
    mapfile -t CARGO_UNIT_FILTERS < <(printf '%s\n' "${CARGO_UNIT_FILTERS[@]}" | sort -u)
    for filter in "${CARGO_UNIT_FILTERS[@]}"; do
      CARGO_OUT=""
      if ! CARGO_OUT=$(run_with_timeout "cargo test $filter" cargo test "$filter" --quiet 2>&1); then
        FAILURES+="cargo test $filter:"$'\n'"$CARGO_OUT"$'\n\n'
      fi
    done
  fi

  if [ ${#CARGO_TEST_TARGETS[@]} -gt 0 ]; then
    mapfile -t CARGO_TEST_TARGETS < <(printf '%s\n' "${CARGO_TEST_TARGETS[@]}" | sort -u)
    for target in "${CARGO_TEST_TARGETS[@]}"; do
      CARGO_OUT=""
      if ! CARGO_OUT=$(run_with_timeout "cargo test --test $target" cargo test --test "$target" --quiet 2>&1); then
        FAILURES+="cargo test --test $target:"$'\n'"$CARGO_OUT"$'\n\n'
      fi
    done
  fi
fi

[ -n "$FAILURES" ] && emit_block "$FAILURES"
exit 0
