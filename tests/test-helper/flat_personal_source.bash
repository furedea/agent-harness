# Build a complete flat source from the repository's legacy personal files.
make_flat_personal_source() {
  PERSONAL_SOURCE="$BATS_TEST_TMPDIR/personal-source"
  mkdir -p "$PERSONAL_SOURCE"

  cp "$REPO_ROOT/agents/AGENTS.md" "$PERSONAL_SOURCE/AGENTS.md"
  cp "$REPO_ROOT/agents/command_permissions.json" "$PERSONAL_SOURCE/command_permissions.json"
  cp "$REPO_ROOT/agents/hooks.json" "$PERSONAL_SOURCE/hooks.json"
  cp "$REPO_ROOT/agents/skill_rendering.json" "$PERSONAL_SOURCE/skill_rendering.json"
  cp -R "$REPO_ROOT/agents/hooks" "$PERSONAL_SOURCE/hooks"
  cp -R "$REPO_ROOT/agents/skills" "$PERSONAL_SOURCE/skills"
  cp -R "$REPO_ROOT/claude" "$PERSONAL_SOURCE/claude"
  cp -R "$REPO_ROOT/codex" "$PERSONAL_SOURCE/codex"
  printf '%s\n' '{"version":1,"runtime_commands":[]}' >"$PERSONAL_SOURCE/manifest.json"
}
