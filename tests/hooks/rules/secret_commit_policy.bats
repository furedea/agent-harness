#!/usr/bin/env bats
# Validate the secret commit policy schema and POSIX ERE patterns.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  POLICY="$REPO_ROOT/profiles/furedea/agents/hooks/rules/secret_commit_policy.json"
}

@test "secret commit policy has the supported schema" {
  jq -e '
    type == "object" and
    .version == 1 and
    (.rules | type == "array" and length > 0 and all(.[];
      type == "object" and
      (.pattern | type == "string" and length > 0) and
      (.reason | type == "string" and length > 0)
    ))
  ' "$POLICY"
}

@test "every secret commit path pattern is a valid POSIX ERE" {
  while IFS= read -r _pattern; do
    local _status=0
    grep -E "$_pattern" </dev/null >/dev/null 2>&1 || _status=$?
    if [[ "$_status" -gt 1 ]]; then
      echo "Invalid POSIX ERE: $_pattern"
      return 1
    fi
  done < <(jq -r '.rules[].pattern' "$POLICY")
}
