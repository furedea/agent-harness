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
(`~/.gemini/config/mcp_config.json`) use dedicated files that `devin mcp add -s user` and the
Antigravity MCP manager also write, Hermes keeps `mcp_servers` in `~/.hermes/config.yaml`, and each
names a remote server's endpoint differently. Pi has no MCP client, and dsh reaches MCP servers
only through a separate client plugin that each profile installs. Until now agent-harness rendered no servers, and users merged them in by hand or with
activation scripts. Where should the declaration live, where does each provider's rendering go,
and what happens to servers the user added through the agent?

## Considered Options

- Claude Code location:
    - Merge into `mcpServers` of `~/.claude.json`.
    - `managed-mcp.json` or the `managedMcpServers` managed setting.
    - A `--mcp-config` file passed by a launcher.
    - A skills-directory plugin, `~/.claude/skills/agent-harness/.claude-plugin/plugin.json`.
- Declaration: a source file, a Nix option, or a source file the Nix option can override.
- Devin and Antigravity files: leave them to a later change, or sync each as a second config of
    the provider.
- dsh: insert client plugin rows into the harness's own bridge bundle patch, or render nothing.

## Decision Outcome

Chosen option: "a skills-directory plugin for Claude Code, a tracked mapping in the provider's own
file for Codex, Hermes, Devin, and Antigravity, declared in the source's `mcp_servers.json` that the
Home Manager `mcpServers` option can override by name", because the plugin is the only user-level
Claude Code location that loads without a launcher, accepts command servers, and is a file Claude
Code reads in place and never writes, so rendering cannot race the running agent; and because the
one sync engine already owns parts of a file with tracked keys
([ADR-0029](0029_declare_config_ownership_and_sync_with_one_engine.md)), so the same claim keeps the
servers the user or the agent's CLI added in every other provider. `~/.claude.json` is rewritten by
Claude Code itself. `managed-mcp.json` needs administrator rights and takes exclusive control, so
every server the user added stops loading; `managedMcpServers` is read only from managed settings
and accepts remote servers only. A `--mcp-config` file works only for sessions started through a
wrapper.

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
- Hermes: `mcp_servers` is a `TrackedKeys` claim of the config the harness already syncs into
    `config.yaml`, and the rendered `managed-config.yaml` carries the servers.
- Devin and Antigravity: the provider's own MCP file is a second `ConfigSpec` of the provider,
    rendered from the servers and synced by `sync-config --provider <id> --file mcp-servers`, so
    the sync engine, the sidecar, and the Home Manager activation are the ones every config uses.
    Its only claim is the tracked `mcpServers` mapping, so servers the CLI added and every other key
    stay. A missing file is created.
- Each provider declares the form it reads a remote server in: `type: "http"` with `url` for
    Claude Code, `url` for Codex and Hermes, `url` with `transport: "http"` for Devin, and
    `serverUrl` for Antigravity. A command server is `command`, `args`, and `env` everywhere.
- Pi gets no servers: it has no MCP client, and the harness installs no MCP extension for it. dsh
    gets none either: its client plugin is not part of dsh, must match the dsh version, and is
    installed per profile by the user, so plugin rows the harness wrote would name a package that
    may be absent and break the profile.

### Consequences

- Good, because no activation script edits `~/.claude.json` and the Claude Code rendering has no
    ownership to declare: the file is generated whole.
- Good, because the same declaration reaches every provider with an MCP client the harness can
    configure: Claude Code, Codex, Devin, Hermes, and Antigravity.
- Bad, because Claude Code names the plugin's tools `mcp__plugin_agent-harness_<server>__<tool>`
    instead of `mcp__<server>__<tool>`, so permission rules written for user-scope servers must
    change when a server moves into the source.
- Bad, because a skill named `agent-harness` can no longer be installed while servers are declared,
    and an administrator setting that blocks skills-directory plugins also blocks these servers.
- Bad, because the Devin and Antigravity syncs create `mcp_config.json` (and a sidecar) even when
    the source declares no servers.
- Bad, because Pi and dsh users still declare servers in those agents.
