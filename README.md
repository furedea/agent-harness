# agent-harness

Shared AI agent harness files and installer for Codex and Claude Code.

This repository owns reusable agent assets such as skills, hooks, command policy inputs, and base configuration. The Rust CLI generates provider-specific files and synchronizes the managed part of mutable tool-owned configuration files.

## Commands

```bash
agent-harness install --prefix "$HOME"
agent-harness generate-claude-settings \
  --output "$HOME/.claude/settings.json"
agent-harness generate-claude-hooks \
  --output /tmp/claude-hooks.json
agent-harness generate-codex-config-source \
  --output /tmp/codex-config-source.toml
agent-harness generate-codex-config-fragment \
  --output /tmp/codex-config-fragment.toml
agent-harness generate-codex-hooks \
  --output "$HOME/.codex/hooks.json"
agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"
agent-harness generate-forbidden-commands \
  --output "$HOME/.claude/hooks/rules/forbidden_commands.json"
agent-harness generate-skills \
  --provider codex \
  --output "$HOME/.codex/skills"
agent-harness sync-codex-config \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"
agent-harness verify --prefix "$HOME"
```

## Nix

The flake exposes the Rust CLI as the default package and a Home Manager module. The module links static source files directly and builds generated files as file-level Nix store outputs before exposing them under `~/.codex` and `~/.claude`.

`--source` is optional for commands that render harness assets. By default the CLI uses packaged assets from the Nix output, an embedded Cargo-install fallback, or the current directory when it is an agent-harness source tree. Use `--source <path>` or `AGENT_HARNESS_SOURCE=<path>` only when rendering from a local checkout or custom source tree.
