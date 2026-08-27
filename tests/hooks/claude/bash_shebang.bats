#!/usr/bin/env bats
# Shebang tests for distributed Bash scripts.

setup() {
  load test-helper/setup
}

@test "all distributed shell scripts resolve bash from PATH" {
  while IFS= read -r script; do
    [ "$(head -n 1 "$script")" = "#!/usr/bin/env bash" ] || {
      echo "Unexpected Bash shebang: $script"
      return 1
    }
  done < <(find "$REPO_ROOT/profiles" -type f -name '*.sh' -print)
}
