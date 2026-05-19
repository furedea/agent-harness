#!/usr/bin/env bats
# Validate that every JSON file in the repository parses correctly.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
}

@test "all JSON files are valid" {
  local invalid=()

  while IFS= read -r f; do
    if ! jq empty "$f" 2>/dev/null; then
      invalid+=("$f")
    fi
  done < <(find "$REPO_ROOT" -name '*.json' \
    -not -path '*/.git/*' \
    -not -path '*/node_modules/*' \
    -not -path '*/kawasemi4/*')

  if [ ${#invalid[@]} -gt 0 ]; then
    printf 'Invalid JSON:\n' >&2
    printf '  %s\n' "${invalid[@]}" >&2
    return 1
  fi
}
