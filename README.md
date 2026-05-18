# agent-harness

Shared AI agent harness files and installer for Codex and Claude Code.

This repository owns reusable agent assets such as skills, hooks, command policy inputs, and base configuration. The Rust CLI renders those assets into provider-specific directories and synchronizes the managed part of mutable tool-owned configuration files.

## Commands

```bash
agent-harness render --source . --out /tmp/agent-harness-rendered
agent-harness install --source . --home "$HOME" --mode copy
agent-harness sync-codex-config \
  --source /tmp/agent-harness-rendered/codex/config-source.toml \
  --target "$HOME/.codex/config.toml"
agent-harness verify --home "$HOME"
```

## Nix

The flake exposes the Rust CLI as the default package and a Home Manager module. The module builds the package, runs `agent-harness render` in the Nix build, and links rendered files into `~/.codex` and `~/.claude`.
