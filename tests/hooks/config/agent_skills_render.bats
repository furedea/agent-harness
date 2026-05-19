#!/usr/bin/env bats
# Validate provider-specific Agent Skills rendering from common sources.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
}

render_skills() {
  local provider="$1"
  local output="$2"

  cargo run --quiet -- generate-skills \
    --source "$REPO_ROOT" \
    --provider "$provider" \
    --output "$output"
}

@test "source skill frontmatter contains only common Agent Skills fields" {
  bad=()

  while IFS= read -r path; do
    keys="$(awk '
      /^---$/ {
        marker += 1
        if (marker == 2) {
          exit
        }
        next
      }
      marker == 1 { print }
    ' "$path" |
      sed -nE 's/^([A-Za-z0-9_-]+):.*/\1/p' |
      sort -u)"

    while IFS= read -r key; do
      case "$key" in
        name | description) ;;
        *) bad+=("$path: $key") ;;
      esac
    done <<<"$keys"
  done < <(find "$REPO_ROOT/agents/skills" -mindepth 2 -maxdepth 2 -name SKILL.md | sort)

  if [ "${#bad[@]}" -gt 0 ]; then
    printf '%s\n' "${bad[@]}" >&2
    return 1
  fi
}

@test "renderer adds provider-specific skill frontmatter" {
  render_skills claude "$BATS_TEST_TMPDIR/claude"
  render_skills codex "$BATS_TEST_TMPDIR/codex"

  grep -q 'argument-hint: "{direct | pr-per-feature}"' \
    "$BATS_TEST_TMPDIR/claude/git-commit-split/SKILL.md"
  grep -q 'argument-hint: "{direct | pr-per-feature}"' \
    "$BATS_TEST_TMPDIR/codex/git-commit-split/SKILL.md"
  grep -q 'disable-model-invocation: true' \
    "$BATS_TEST_TMPDIR/claude/skill-auditor/SKILL.md"
}

@test "command-style skills disable auto-trigger on both providers" {
  render_skills claude "$BATS_TEST_TMPDIR/claude"
  render_skills codex "$BATS_TEST_TMPDIR/codex"

  for skill in git-commit-split github-ci-init nix-dev-init skill-auditor; do
    grep -q 'disable-model-invocation: true' \
      "$BATS_TEST_TMPDIR/claude/$skill/SKILL.md"
    grep -q 'allow_implicit_invocation: false' \
      "$BATS_TEST_TMPDIR/codex/$skill/agents/openai.yaml"
    [ ! -e "$BATS_TEST_TMPDIR/claude/$skill/agents/openai.yaml" ]
  done
}
