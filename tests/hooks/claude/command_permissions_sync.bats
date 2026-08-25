#!/usr/bin/env bats
# Tests that generated Bash permissions, shared prefixes, and precise regex rules stay aligned.

setup() {
  load test-helper/setup
  COMMAND_PERMISSIONS="$REPO_ROOT/profiles/furedea/agents/command_permissions.json"
  ALLOWED_RULES="$REPO_ROOT/profiles/furedea/agents/hooks/rules/allowed_commands.json"
}

read_settings() {
  local settings

  settings="$BATS_TEST_TMPDIR/generated_settings.json"
  cargo run --quiet -- generate-claude-settings \
    --profile furedea \
    --source "$REPO_ROOT" \
    --output "$settings"
  jq "$@" "$settings"
}

get_settings_allow_prefixes() {
  read_settings -r '.permissions.allow[]' |
    grep '^Bash(' |
    sed -E 's/^Bash\(([^:]+):\*\)$/\1/'
}

get_policy_allow_prefixes() {
  jq -r '.rules[] | select(.decision == "allow") | .prefix | join(" ")' "$COMMAND_PERMISSIONS"
}

get_allowed_patterns() {
  jq -r '.rules[].patterns[]' "$ALLOWED_RULES"
}

assert_lines_contain() {
  local _expected="$1"
  local _actual="$2"
  local _label="$3"
  local _missing=()
  local _line

  while IFS= read -r _line; do
    if ! echo "$_actual" | grep -qxF "$_line"; then
      _missing+=("$_line")
    fi
  done <<<"$_expected"

  if [ ${#_missing[@]} -gt 0 ]; then
    echo "$_label"
    printf '  - %s\n' "${_missing[@]}"
    return 1
  fi
}

@test "every generated Bash allow has a shared allow prefix" {
  assert_lines_contain \
    "$(get_settings_allow_prefixes)" \
    "$(get_policy_allow_prefixes)" \
    "Generated Bash allows missing from agents/command_permissions.json:"
}

@test "every shared allow prefix is generated as a Bash allow" {
  assert_lines_contain \
    "$(get_policy_allow_prefixes)" \
    "$(get_settings_allow_prefixes)" \
    "Shared allow prefixes missing from generated Claude permissions:"
}

@test "every shared allow prefix has at least one global regex" {
  local missing=()
  local patterns
  local prefix
  local escaped

  patterns=$(get_allowed_patterns)
  while IFS= read -r prefix; do
    # shellcheck disable=SC2016
    escaped=$(printf '%s' "$prefix" | sed 's/[.[\*^$()+?{|]/\\&/g')
    if ! echo "$patterns" | grep -qE "^\^${escaped}( |\(|\$|\\\\s)"; then
      missing+=("$prefix")
    fi
  done <<<"$(get_policy_allow_prefixes)"

  if [ ${#missing[@]} -gt 0 ]; then
    echo "Shared allow prefixes missing from agents/hooks/rules/allowed_commands.json:"
    printf '  - %s\n' "${missing[@]}"
    return 1
  fi
}
