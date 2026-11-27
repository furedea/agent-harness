---
status: accepted
date: 2026-11-27
---

# Render MCP servers from one declaration

## Context and Problem Statement

Each agent reads MCP servers from its own file, and several of those files are also written by
the agent. Claude Code keeps user-scope servers in `~/.claude.json`, which it rewrites throughout
a session, so a configuration tool that merges servers into it at activation can lose either its
own write or Claude Code's. Codex keeps them in `config.toml` as `[mcp_servers.<name>]`, which
`codex mcp add` also writes. Devin (`~/.config/devin/mcp_config.json`) and Antigravity
(`~/.gemini/config/mcp_config.json`) use dedicated files their CLIs also write, and Pi has no MCP
support. Until now agent-harness rendered no servers, and users merged them in by hand or with
activation scripts. Where should the declaration live, where does each provider's rendering go,
and what happens to servers the user added through the agent?

## Considered Options

- Claude Code location:
    - Merge into `mcpServers` of `~/.claude.json`.
    - `managed-mcp.json` or the `managedMcpServers` managed setting.
    - A `--mcp-config` file passed by a launcher.
    - A skills-directory plugin, `~/.claude/skills/agent-harness/.claude-plugin/plugin.json`.
- Declaration: a source file, a Nix option, or a source file the Nix option can override.

## Decision Outcome

Chosen option: "a skills-directory plugin for Claude Code, the tracked `mcp_servers` table for
Codex, declared in the source's `mcp_servers.json` that the Home Manager `mcpServers` option can
override by name", because the plugin is the only user-level Claude Code location that loads
without a launcher, accepts command servers, and is a file Claude Code reads in place and never
writes, so rendering cannot race the running agent; and because Codex already syncs `config.toml`
with tracked keys ([ADR-0029](0029_declare_config_ownership_and_sync_with_one_engine.md)), so the
same claim keeps the servers the user added. `~/.claude.json` is rewritten by Claude Code itself.
`managed-mcp.json` needs administrator rights and takes exclusive control, so every server the user
added stops loading; `managedMcpServers` is read only from managed settings and accepts remote
servers only. A `--mcp-config` file works only for sessions started through a wrapper.

- The source declares `{"servers": {<name>: {"command", "args", "env"} | {"url"}}}`. The format is
    host-neutral; a command may be a name on `PATH`. The Home Manager option exists for values only
    the host knows, such as a Nix store path, and replaces a source entry of the same name whole, as
    `claude.settings` overrides the source settings.
- The harness renders the values as written and keeps no credentials
    ([ADR-0032](0032_limit_the_harness_to_rendering_and_installing.md)). A server that needs one
    reads it from its own environment, or its `command` is a wrapper that fetches it and starts the
    server.
- Claude Code: the plugin is part of the rendered skills directory, which installation already
    replaces whole, so `generate-skills --provider <id>` renders a provider's own skills directory.
    Servers the user added with `claude mcp add` stay in `~/.claude.json`, which the harness never
    touches.
- Codex: `mcp_servers` is a `TrackedKeys` claim in the existing sidecar. A server the source
    dropped is removed, and one the user or `codex mcp add` added stays. A name declared in both
    `mcp_servers.json` and the source `codex/config.toml` is an error.
- Devin and Antigravity are not rendered yet: each needs a second synced file per provider, which
    the provider spec and the `sync-config` command do not model. Pi, Hermes, and dsh are out of
    scope.

### Consequences

- Good, because no activation script edits `~/.claude.json` and the Claude Code rendering has no
    ownership to declare: the file is generated whole.
- Good, because the same declaration reaches Claude Code and Codex.
- Bad, because Claude Code names the plugin's tools `mcp__plugin_agent-harness_<server>__<tool>`
    instead of `mcp__<server>__<tool>`, so permission rules written for user-scope servers must
    change when a server moves into the source.
- Bad, because a skill named `agent-harness` can no longer be installed while servers are declared,
    and an administrator setting that blocks skills-directory plugins also blocks these servers.
- Bad, because Devin and Antigravity users still declare servers in those agents.
