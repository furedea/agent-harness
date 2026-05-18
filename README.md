# agent-harness

Shared AI agent harness files and installer for Codex and Claude Code.

This repository owns reusable agent assets such as skills, hooks, command policy inputs, and base configuration. The Rust CLI generates provider-specific files and synchronizes the managed part of mutable tool-owned configuration files.

## Commands

```bash
agent-harness install --source . --prefix "$HOME" --mode copy
agent-harness generate-claude-settings \
  --source . \
  --output "$HOME/.claude/settings.json"
agent-harness generate-codex-config-source \
  --source . \
  --output /tmp/codex-config-source.toml
agent-harness generate-codex-rules \
  --source . \
  --output "$HOME/.codex/rules/default.rules"
agent-harness generate-forbidden-commands \
  --source . \
  --output "$HOME/.claude/rules/forbidden_commands.json"
agent-harness generate-skills \
  --source . \
  --provider codex \
  --output "$HOME/.codex/skills"
agent-harness sync-codex-config \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"
agent-harness verify --prefix "$HOME"
```

## Nix

The flake exposes the Rust CLI as the default package and a Home Manager module. The module links static source files directly and builds generated files as file-level Nix store outputs before exposing them under `~/.codex` and `~/.claude`.
