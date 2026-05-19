#!/usr/bin/env bats
# Validate that every regex pattern in secret_content_patterns.json compiles.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  PATTERNS="$REPO_ROOT/agents/hooks/rules/secret_content_patterns.json"
}

@test "patterns file exists and is valid JSON" {
  [ -f "$PATTERNS" ]
  jq empty "$PATTERNS"
}

@test "every pattern has required fields" {
  local missing=()

  while IFS= read -r key; do
    local pattern message
    pattern=$(jq -r --arg k "$key" '.[$k].pattern // empty' "$PATTERNS")
    message=$(jq -r --arg k "$key" '.[$k].message // empty' "$PATTERNS")

    if [ -z "$pattern" ]; then
      missing+=("$key: missing pattern")
    fi
    if [ -z "$message" ]; then
      missing+=("$key: missing message")
    fi
  done < <(jq -r 'keys[]' "$PATTERNS")

  if [ ${#missing[@]} -gt 0 ]; then
    printf 'Incomplete rule:\n' >&2
    printf '  %s\n' "${missing[@]}" >&2
    return 1
  fi
}

@test "every pattern is a valid PCRE2 regex" {
  if ! command -v rg &>/dev/null; then
    skip "rg (ripgrep) not available"
  fi

  local invalid=()

  while IFS= read -r key; do
    local pattern
    pattern=$(jq -r --arg k "$key" '.[$k].pattern // empty' "$PATTERNS")
    [ -z "$pattern" ] && continue

    if ! echo "" | rg --pcre2 -q -e "$pattern" 2>/dev/null; then
      # rg returns 1 for no match, 2 for invalid regex
      if ! echo "" | rg --pcre2 -e "$pattern" 2>/dev/null; [ $? -eq 2 ]; then
        invalid+=("$key: $pattern")
      fi
    fi
  done < <(jq -r 'keys[]' "$PATTERNS")

  if [ ${#invalid[@]} -gt 0 ]; then
    printf 'Invalid regex:\n' >&2
    printf '  %s\n' "${invalid[@]}" >&2
    return 1
  fi
}
