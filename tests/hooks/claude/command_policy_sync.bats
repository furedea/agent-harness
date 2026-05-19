#!/usr/bin/env bats
# Tests that every Bash permission in generated settings has matching coverage
# in guard_allowed_commands.sh.
#
# Invariant: each Bash(prefix:*) allow entry in generated settings must have
# a corresponding GOVERNED_PREFIXES entry AND at least one ALLOWED_PATTERNS
# entry in guard_allowed_commands.sh.
#
# This prevents auto-approved commands from bypassing the allowlist hook.

setup() {
  load test-helper/setup
  ALLOWLIST="$REPO_ROOT/agents/hooks/guard_allowed_commands.sh"
}

read_settings() {
  local settings
  settings="$BATS_TEST_TMPDIR/generated_settings.json"
  cargo run --quiet -- generate-claude-settings --source "$REPO_ROOT" --output "$settings"
  jq "$@" "$settings"
}

# Extract Bash allow prefixes from generated settings.
# "Bash(gh api:*)" → "gh api"
get_settings_bash_prefixes() {
  read_settings -r '.permissions.allow[]' |
    grep '^Bash(' |
    sed -E 's/^Bash\(([^:]+):\*\)$/\1/'
}

# Extract GOVERNED_PREFIXES entries from guard_allowed_commands.sh.
get_governed_prefixes() {
  sed -n '/^GOVERNED_PREFIXES=(/,/^)/p' "$ALLOWLIST" |
    grep -E '^[[:space:]]+"' |
    sed -E 's/^[[:space:]]+"([^"]+)"$/\1/'
}

# Extract ALLOWED_PATTERNS entries from guard_allowed_commands.sh (just the raw patterns).
get_allowed_patterns() {
  sed -n '/^ALLOWED_PATTERNS=(/,/^)/p' "$ALLOWLIST" |
    grep -E "^[[:space:]]+['\"]" |
    sed -E "s/^[[:space:]]+['\"]([^'\"]*)['\"].*/\1/"
}

# ============================================================
# Core invariant tests
# ============================================================

@test "every Bash allow in generated settings has a GOVERNED_PREFIXES entry" {
  local missing=()
  local governed
  governed=$(get_governed_prefixes)

  while IFS= read -r prefix; do
    if ! echo "$governed" | grep -qxF "$prefix"; then
      missing+=("$prefix")
    fi
  done <<<"$(get_settings_bash_prefixes)"

  if [ ${#missing[@]} -gt 0 ]; then
    echo "Bash prefixes in generated settings missing from GOVERNED_PREFIXES:"
    printf '  - %s\n' "${missing[@]}"
    echo ""
    echo "Fix: add the missing prefix(es) to GOVERNED_PREFIXES in"
    echo "  .claude/hooks/guard_allowed_commands.sh"
    return 1
  fi
}

@test "every GOVERNED_PREFIXES entry has a Bash allow in generated settings" {
  local missing=()
  local settings_prefixes
  settings_prefixes=$(get_settings_bash_prefixes)

  while IFS= read -r prefix; do
    if ! echo "$settings_prefixes" | grep -qxF "$prefix"; then
      missing+=("$prefix")
    fi
  done <<<"$(get_governed_prefixes)"

  if [ ${#missing[@]} -gt 0 ]; then
    echo "GOVERNED_PREFIXES entries missing from generated settings Bash allows:"
    printf '  - %s\n' "${missing[@]}"
    echo ""
    echo "Fix: add Bash(<prefix>:*) to permissions.allow in"
    echo "  nix/agents/command_policy.nix, or remove the prefix from GOVERNED_PREFIXES"
    return 1
  fi
}

@test "every GOVERNED_PREFIXES entry has at least one ALLOWED_PATTERNS entry" {
  local missing=()
  local patterns
  patterns=$(get_allowed_patterns)

  while IFS= read -r prefix; do
    # Escape regex metacharacters in prefix for grep
    local escaped
    # shellcheck disable=SC2016
    escaped=$(printf '%s' "$prefix" | sed 's/[.[\*^$()+?{|]/\\&/g')
    # Check if any pattern starts with ^ followed by the prefix and a delimiter
    if ! echo "$patterns" | grep -qE "^\^${escaped}( |\(|\$|\\\\s)"; then
      missing+=("$prefix")
    fi
  done <<<"$(get_governed_prefixes)"

  if [ ${#missing[@]} -gt 0 ]; then
    echo "GOVERNED_PREFIXES entries with no matching ALLOWED_PATTERNS:"
    printf '  - %s\n' "${missing[@]}"
    echo ""
    echo "Fix: add at least one allowed regex pattern for each prefix in"
    echo "  .claude/hooks/guard_allowed_commands.sh"
    return 1
  fi
}
