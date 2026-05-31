#!/usr/bin/env bats
# Validates related-test rule files. The global default rules describe
# default source-to-test conventions; project extension rules add fan-out
# mappings for this repository.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  RULES="$REPO_ROOT/agents/hooks/rules/related_test_extensions.json"
  LANGUAGE_RULES="$REPO_ROOT/agents/hooks/rules/related_test_defaults.json"
}

@test "project extension rules file is valid JSON" {
  jq empty "$RULES"
}

@test "default rules file is valid JSON" {
  jq empty "$LANGUAGE_RULES"
}

@test "default rules define repository test conventions" {
  jq -e '
    .bats.source_extensions == [".sh", ".bats"]
      and (.bats.test_patterns | index("{stem}.bats"))
      and (.bats.test_patterns | index("test_{stem}.bats"))
      and .python.source_extensions == [".py"]
      and (.python.test_patterns | index("test_{stem}.py"))
      and (.python.test_patterns | index("{stem}_test.py"))
      and (.python.project_markers | index("pyproject.toml"))
      and .rust.source_extensions == [".rs"]
      and (.rust.project_markers | index("Cargo.toml"))
      and .rust.integration_test_dir == "tests"
      and (.rust.source_dirs | index("src"))
      and (.rust.skip_unit_filter_stems | index("lib"))
      and (.rust.skip_unit_filter_stems | index("main"))
      and (.rust.skip_unit_filter_stems | index("mod"))
      and .rust.strategy == "unit_filter_and_integration_targets"
      and .javascript_typescript.source_extensions == [".js", ".jsx", ".ts", ".tsx"]
      and (.javascript_typescript.test_patterns | index("{stem}.test.ts"))
      and (keys | sort) == ["bats", "javascript_typescript", "python", "rust"]
  ' "$LANGUAGE_RULES" >/dev/null
}

@test "default rules only cover test-selection families" {
  jq -e '
    [.[].lint_hook] | sort == [
      "lint_format_js.sh",
      "lint_format_py.sh",
      "lint_format_rs.sh",
      "lint_format_sh.sh"
    ]
  ' "$LANGUAGE_RULES" >/dev/null
}

@test "every project extension value is a non-empty array of strings" {
  jq -e 'to_entries | all(.value | type == "array" and length > 0 and all(. | type == "string"))' "$RULES" >/dev/null
}

@test "every project extension test file exists" {
  cd "$REPO_ROOT"
  missing=()
  while IFS= read -r t; do
    [ -f "$t" ] || missing+=("$t")
  done < <(jq -r 'values[] | .[]' "$RULES" | sort -u)
  if [ "${#missing[@]}" -gt 0 ]; then
    printf 'missing test files referenced by rules:\n' >&2
    printf '  %s\n' "${missing[@]}" >&2
    return 1
  fi
}

@test "library files fan out to their consumers" {
  audit=$(jq -r '."agents/hooks/lib/audit_log.sh"[]' "$RULES")
  [[ "$audit" == *guard_allowed_commands.bats* ]]
  [[ "$audit" == *guard_dangerous_git.bats* ]]

  parse=$(jq -r '."agents/hooks/lib/shell_parse.sh"[]' "$RULES")
  [[ "$parse" == *guard_allowed_commands.bats* ]]
  [[ "$parse" == *adapt_shell_command.bats* ]]

  lint=$(jq -r '."agents/hooks/lib/lint_format.sh"[]' "$RULES")
  [[ "$lint" == *lint_format_py.bats* ]]
  [[ "$lint" == *adapt_lint_format.bats* ]]
}

@test "glob pattern keys cover lint_format hooks" {
  result=$(jq -r '."agents/hooks/lint_format_*.sh"[]' "$RULES")
  [[ "$result" == *lint_format_hooks.bats* ]]
}

@test "secret-content patterns trigger both Claude and Codex tests" {
  result=$(jq -r '."agents/hooks/guard_secret_content.sh"[]' "$RULES")
  [[ "$result" == *guard_secret_content.bats* ]]
  [[ "$result" == *adapt_guard_secret_content.bats* ]]
}

@test "secret path policy triggers generated settings and Codex hook tests" {
  result=$(jq -r '."agents/secret_path_policy.json"[]' "$RULES")
  [[ "$result" == *tests/generated_artifacts.rs* ]]
  [[ "$result" == *adapt_guard_secret_paths.bats* ]]

  code=$(jq -r '."src/generation/secret_path_policy.rs"[]' "$RULES")
  [[ "$code" == *tests/generated_artifacts.rs* ]]
  [[ "$code" == *adapt_guard_secret_paths.bats* ]]
}

@test "codex config source fans out to generated config tests" {
  result=$(jq -r '."src/codex_config.rs"[]' "$RULES")
  [[ "$result" == *tests/generated_artifacts.rs* ]]

  data=$(jq -r '."codex/config.toml"[]' "$RULES")
  [[ "$data" == *tests/generated_artifacts.rs* ]]
}

@test "hook configuration fans out to generated provider tests" {
  config=$(jq -r '."agents/hooks.json"[]' "$RULES")
  [[ "$config" == *tests/generated_artifacts.rs* ]]

  code=$(jq -r '."src/hooks.rs"[]' "$RULES")
  [[ "$code" == *tests/generated_artifacts.rs* ]]
}

@test "settings.base.json triggers lock tests and allowlist" {
  result=$(jq -r '."claude/settings.base.json"[]' "$RULES")
  [[ "$result" == *tests/generated_artifacts.rs* ]]
  [[ "$result" == *guard_allowed_commands.bats* ]]
}

@test "command policy data triggers generated settings tests on both providers" {
  result=$(jq -r '."agents/command_policy.json"[]' "$RULES")
  [[ "$result" == *tests/generated_artifacts.rs* ]]
  [[ "$result" == *tests/codex/execpolicy.bats* ]]
  [[ "$result" == *guard_allowed_commands.bats* ]]

  code=$(jq -r '."src/command_policy.rs"[]' "$RULES")
  [[ "$code" == *tests/generated_artifacts.rs* ]]
  [[ "$code" == *tests/codex/execpolicy.bats* ]]
  [[ "$code" == *guard_allowed_commands.bats* ]]
}

@test "agents/skills/* triggers skills render test" {
  result=$(jq -r '."agents/skills/*"[]' "$RULES")
  [ "$result" = "tests/generated_artifacts.rs" ]

  code=$(jq -r '."src/skills.rs"[]' "$RULES")
  [ "$code" = "tests/generated_artifacts.rs" ]
}
