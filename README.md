# agent-harness

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/lang-English-0969da"></a>
  <a href="README_ja.md"><img alt="日本語" src="https://img.shields.io/badge/lang-%E6%97%A5%E6%9C%AC%E8%AA%9E-8c959f"></a>
</p>

**Share hooks—not just skills—across coding agents.**

`agent-harness` is a Nix-native composition layer for hooks, skills, agent instructions, command
permissions, and provider settings. Define one environment, then render the native files expected
by Claude Code, Codex, Devin, Hermes Agent, pi, and DeepSeek Harness (dsh).

The packaged default stays deliberately small. Personal or team behavior lives in a complete
source tree beside the rest of its configuration and can compose local or Nix-built skills and
hook bundles without forking this repository.

```text
AGENTS.md and command permissions
Local or command-generated skills
Local or command-generated hook bundles
Provider settings
              |
              v
        agent-harness
        |-- Claude Code: settings, hooks, and skills
        |-- Codex: config, hooks, adapters, rules, and skills
        |-- Devin: config, hooks, and command permissions
        |-- Hermes Agent: hook manifest, plugin, and config
        |-- pi: hook manifest, bridge extension, settings, and skills
        |-- dsh: hook manifest, bridge bundle, and skills
        `-- antigravity: hook manifest, skills, AGENTS.md, and generated settings permissions
```

## Why

- **Shared hooks**: reuse hook behavior across Claude Code, Codex, Devin, Hermes Agent, pi, and dsh,
    including tools whose CLI normally writes directly into each provider's config directory.
- **One set of guardrails**: generate Claude Bash permissions, Codex execpolicy, and Devin Exec
    permissions from one neutral `allow` / `ask` / `deny` source.
- **Composable extensions**: combine built-in behavior with external skills and hook bundles.
- **CLI-to-Nix bridge**: turn command output into a skill, or capture supported files created by
    hook installers inside an isolated build home.
- **Provider-native output**: render the adapters, hook wiring, permissions, rules, and
    configuration expected by each agent.
- **State-aware config sync**: replace harness-owned keys while preserving tool-owned and
    user-owned state in the Claude Code, Codex, Devin, Hermes Agent, pi, and dsh configs.

For the problems this design addresses, the core decisions, and the alternatives considered, see
[docs/DESIGN.md](docs/DESIGN.md).

## Sources

The built-in `minimal` source contains a neutral `AGENTS.md`, empty skill, hook, and permission
sets, and no model defaults. It exists as a usable starting point and as the CLI default. It does
not contain the repository author's personal hooks or skills.

A customized environment is a complete flat source tree passed with `--source` or configured as
the Home Manager module's `source`. Its `manifest.json` declares required runtime commands, and
`verify` reports any that are missing. This keeps personal policy changes in the repository where
they are owned while agent-harness remains a reusable renderer and deployment boundary.

## Compose with Nix

The Home Manager module accepts ordinary paths and derivations. A personal setup can therefore live
next to the rest of a user's Nix configuration:

```text
dotfiles/
└── agents/
    ├── manifest.json
    ├── AGENTS.md
    ├── command_permissions.json
    ├── hooks.json
    ├── skill_rendering.json
    ├── hooks/
    ├── skills/
    ├── claude/
    └── codex/
```

```nix
programs.agent-harness = {
  enable = true;
  source = ./agents;

  skills = {
    team-workflows = ./agent-sources/skills/team-workflows;
  };

  hooks = {
    notifications = ./agent-sources/hook-bundles/notifications;
  };
};
```

The flake also exposes builders for integrations owned by other CLIs. [Herdr](https://herdr.dev/)
for example, prints a release-matched skill with `herdr --skill` and installs provider hooks with
`herdr integration install`:

```nix
let
  harnessLib = agent-harness.lib.${pkgs.system};

  herdrSkill = harnessLib.buildSkillFromCommand {
    name = "herdr";
    command = [
      "${herdr}/bin/herdr"
      "--skill"
    ];
  };

  herdrHooks = harnessLib.buildHookBundleFromCommands {
    name = "herdr";
    commands = [
      [ "${herdr}/bin/herdr" "integration" "install" "claude" ]
      [ "${herdr}/bin/herdr" "integration" "install" "codex" ]
    ];
  };
in
{
  programs.agent-harness = {
    enable = true;
    skills.herdr = herdrSkill;
    hooks.herdr = herdrHooks;
  };
}
```

`buildSkillFromCommand` writes the command's standard output to `SKILL.md` and rejects empty output.
`buildHookBundleFromCommands` runs commands in order with an isolated `HOME`, captures supported
provider hook artifacts, and writes a versioned bundle. Each command is an argument list: the
first item is the executable and the remaining items are passed as arguments without a shell.
Hook installer executables must be absolute; absolute Nix package paths are recommended for skill
commands as well. Builds should be deterministic and must not depend on interactive input or
network access.

The hook builder captures each provider's hook file (`.claude/settings.json`, `.codex/hooks.json`,
`.devin/hooks.v1.json`, `.hermes/hooks.json`, `.pi/hooks.json`, `.dsh/hooks.json`) and hook
directory (`.claude/hooks/`, `.codex/hooks/`, `.devin/hooks/`, `.hermes/hooks/`, `.pi/hooks/`,
`.dsh/hooks/`), plus `.codex/config.toml` and top-level `.codex/*.sh` files. If generated hook commands
embed a build-time path that must differ at runtime, pass
`commandReplacements = [{ from = "..."; to = "..."; }]`.

For installers such as [`moshi-hook install`](https://getmoshi.app/docs/hooks), use the same hook
builder. Pairing credentials and daemon state are runtime concerns and are not captured in the
bundle:

```nix
moshiHooks = agent-harness.lib.${pkgs.system}.buildHookBundleFromCommands {
  name = "moshi";
  commands = [ [ "${moshiHook}/bin/moshi-hook" "install" ] ];
};
```

### GitHub credentials

agent-harness does not supply or refresh a GitHub token
([ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)). To let agents run
`gh` and `git push` with their own token inside the sandbox, write the wiring in the source
settings, which the harness renders as they are: for Claude, `env.GH_CONFIG_DIR`, the GitHub hosts
in `sandbox.network.allowedDomains`, `sandbox.enableWeakerNetworkIsolation`, no `gh *` in
`sandbox.excludedCommands`, and `Read(...)` and `sandbox.filesystem.denyRead` denials of any key
that must stay unreadable; for Codex, `shell_environment_policy.set.GH_CONFIG_DIR` and a permissions
profile that can read that directory. Mint and refresh the token with a tool outside the harness.

### MCP servers

Declare MCP servers once in the source's optional `mcp_servers.json`. Each server is a command
(`command`, optional `args` and `env`) or a remote endpoint (`url`, streamable HTTP):

```json
{
  "servers": {
    "github": {
      "command": "github-mcp-server",
      "args": ["stdio"],
      "env": { "GITHUB_TOOLSETS": "repos,issues" }
    },
    "docs": { "url": "https://docs.example.com/mcp" }
  }
}
```

A command may be a name on `PATH`, which keeps the source host-neutral. For a host-specific value
such as a Nix store path, the Home Manager option `mcpServers` adds entries or replaces the
source's entry of the same name whole:

```nix
programs.agent-harness.mcpServers.github = {
  command = "${pkgs.github-mcp-server}/bin/github-mcp-server";
  args = [ "stdio" ];
};
```

The harness writes the values as they are into provider files, so never put a credential in them.
A server that needs one reads it from its own environment, or its `command` is a wrapper script
that fetches the credential (for example from the system keychain) and starts the server.

| Provider    | Rendered into                                                          | A remote server is written as     | Servers you added yourself                           |
| ----------- | ---------------------------------------------------------------------- | --------------------------------- | ---------------------------------------------------- |
| Claude Code | the plugin `~/.claude/skills/agent-harness/.claude-plugin/plugin.json` | `type: "http"`, `url`             | stay in `~/.claude.json`, which is never touched     |
| Codex       | `[mcp_servers.<name>]` in `~/.codex/config.toml`                       | `url`                             | stay; only servers a previous sync wrote are removed |
| Devin       | `mcpServers` in `~/.config/devin/mcp_config.json`                      | `url`, `transport: "http"`        | stay; only servers a previous sync wrote are removed |
| Hermes      | `mcp_servers` in `~/.hermes/config.yaml`                               | `url`                             | stay; only servers a previous sync wrote are removed |
| Antigravity | `mcpServers` in `~/.gemini/config/mcp_config.json`                     | `serverUrl`                       | stay; only servers a previous sync wrote are removed |

Claude Code loads the plugin from the skills directory in place and never writes to it, so a sync
never races the running agent for `~/.claude.json`. It names the tools
`mcp__plugin_agent-harness_<server>__<tool>`, so write permission rules with that prefix. A name in
both `mcp_servers.json` and the source `codex/config.toml` is an error. In the files the harness
syncs, a declared server replaces one you added under the same name whole, and from then on the
harness owns it: rename your own server to keep it.

Devin and Antigravity keep servers in a file of their own that `devin mcp add -s user` and the
Antigravity MCP manager also write. Installation renders it next to the provider's config, and
`sync-config --provider <devin|antigravity> --file mcp-servers` merges it into the real file; the
Home Manager module runs that sync on activation. The servers a sync wrote are recorded in
`.agent-harness-managed.json` next to each target, as for Codex.

Pi has no MCP client of its own, and agent-harness installs no MCP extension for it, so Pi gets no
servers. dsh reaches MCP servers only through the separate `@deepseek-ai/dsh-mcp-client` plugin,
which each dsh profile must install itself at the version that matches dsh; agent-harness does not
install it, so dsh gets no servers either. See
[ADR-0033](docs/adr/0033_render_mcp_servers_from_one_declaration.md).

## Quick Install

Release binaries are currently built for `x86_64-unknown-linux-musl`. The quickest install on that
platform uses the shell installer generated by cargo-dist:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-installer.sh \
  | sh
```

Then install or verify the managed harness files:

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

This installs the neutral built-in `minimal` source. Pass `--source <path>` to render a complete
personal or team source tree instead.

For a source-based Cargo install:

```bash
cargo install --locked --git https://github.com/furedea/agent-harness agent-harness
```

The shell and Cargo installers place the binary under Cargo's bin directory, normally
`$HOME/.cargo/bin`. Make sure that directory is on `PATH`.

For Nix on Apple Silicon macOS:

```bash
nix profile install github:furedea/agent-harness
```

After installing the binary, `agent-harness install` is still required to render the provider
configuration. `verify` checks the required installed paths and the runtime commands
declared by the resolved source manifest.

## Other Installation Methods

### Release Archive

Use this on `x86_64` Linux when you want the binary release but do not want to pipe an installer
into Bash. The current cargo-dist configuration uses its default Unix archive format, `.tar.xz`.

```bash
mkdir -p "$HOME/.local/agent-harness" "$HOME/.local/bin"

curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz
tar -xJf agent-harness-x86_64-unknown-linux-musl.tar.xz \
  -C "$HOME/.local/agent-harness" \
  --strip-components=1

ln -sf "$HOME/.local/agent-harness/agent-harness" "$HOME/.local/bin/agent-harness"

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Cargo

Use this when you already have a Rust toolchain and want to build from the repository.

```bash
cargo install --locked --git https://github.com/furedea/agent-harness agent-harness

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Nix

The flake exposes a package for `x86_64-linux` and `aarch64-darwin`.

```bash
nix run github:furedea/agent-harness -- install --prefix "$HOME"
nix run github:furedea/agent-harness -- verify --prefix "$HOME"
```

### Home Manager

Use the Home Manager module when your agent config is managed by Nix. The module is exposed as
`homeManagerModules.default` and supports `x86_64-linux` and `aarch64-darwin`.

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    agent-harness.url = "github:furedea/agent-harness";
  };

  outputs =
    {
      agent-harness,
      home-manager,
      nixpkgs,
      ...
    }:
    {
      homeConfigurations.example = home-manager.lib.homeManagerConfiguration {
        pkgs = import nixpkgs { system = "aarch64-darwin"; };
        modules = [
          agent-harness.homeManagerModules.default
          {
            programs.agent-harness.enable = true;
          }
        ];
      };
    };
}
```

As shown in [Compose with Nix](#compose-with-nix), additional skill directories and external hook
bundles can be composed without copying them into this repository. Skill directories are rendered for
Claude, Codex, pi, and dsh (Devin and Hermes have no skill destination), and hook bundles apply to
every provider. Skill directories must contain `SKILL.md`. Installed names may contain only lowercase ASCII letters,
digits, and hyphens, and may not shadow a built-in skill. Regular files are copied verbatim;
symlinks are ignored.

Each bundle must contain `hook_bundle.json` with `{"version":1}` and may provide any provider's hook
file and hook scripts, plus Codex feature flags. Scripts are installed below each provider's hook
directory as `hooks/external/<name>/` (for example `~/.claude/hooks/external/<name>/`). Hook JSON
is merged structurally; external Codex configuration may add only `[features]` entries. The CLI
equivalent of `hooks` is `--extra-hook NAME=PATH` on `install` and on the `generate-*` commands that take `--source` and
`--output` (all except `generate-hook-bundle` and `generate-skills`).

The module interface is:

| Option               | Default       | Meaning                                                                     |
| -------------------- | ------------- | --------------------------------------------------------------------------- |
| `enable`             | `false`       | Enable the package and managed files.                                       |
| `package`            | flake package | Select the `agent-harness` executable.                                      |
| `source`             | this flake    | Profile collection or complete flat profile source tree.                    |
| `profile`            | `"minimal"`   | Select the packaged composition base.                                       |
| `agentsMd`           | profile file  | Replace the shared `AGENTS.md` / `CLAUDE.md` source.                        |
| `commandPermissions` | profile file  | Replace provider-neutral `allow` / `ask` / `deny` rules.                    |
| `mcpServers`         | `{}`          | Add MCP servers, or replace the source's servers of the same name.          |
| `skills`             | `{}`          | Add skill directories or derivations by installed name.                     |
| `hooks`              | `{}`          | Add versioned hook bundle directories or derivations by name.               |
| `claude.enable`      | `true`        | Install Claude Code files.                                                  |
| `claude.settings`    | `{}`          | Optionally override keys in the selected Claude base.                       |
| `codex.enable`       | `true`        | Install Codex files.                                                        |
| `codex.settings`     | `{}`          | Optionally override keys in the selected Codex base.                        |
| `devin.enable`       | `true`        | Install Devin files.                                                        |
| `hermes.enable`      | `true`        | Install Hermes Agent files.                                                 |
| `pi.enable`          | `true`        | Install pi files.                                                           |
| `dsh.enable`         | `true`        | Install DeepSeek Harness files.                                             |
| `antigravity.enable` | `true`        | Install Antigravity files (`~/.gemini/config`, `agy` settings permissions). |
| `dsh.profile`        | `"web"`       | User-owned dsh profile that registers the hook bundle.                      |

Generated hook wiring and Bash command permissions remain harness-managed after the provider
settings merge. This prevents a settings overlay from silently removing enforcement. The harness
does not protect the files it installs: write the protection in the source settings, for example a
sandbox write allowlist and one deny per harness root such as `Edit(~/.claude/**)`
([ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)).
Set a provider's `enable` option to `false` to skip its files.

The Home Manager module materializes `~/.claude/settings.json` as a writable regular file because
Claude Code may update it. On activation, generated top-level keys replace the corresponding
existing values, while top-level keys absent from the generated settings are preserved. Other
Claude Code files remain Home Manager-managed symlinks. Activation also syncs the Codex, Devin,
Hermes Agent, pi, and dsh configs as described in [What Gets Installed](#what-gets-installed).

## Optional Integrity Check

Cargo-dist publishes a `.sha256` file for each release archive.

```bash
curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz
curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz.sha256
sha256sum -c agent-harness-x86_64-unknown-linux-musl.tar.xz.sha256
```

## What Gets Installed

`agent-harness install --prefix "$HOME"` writes the rendered harness into the Claude Code, Codex,
Devin, Hermes Agent, pi, and dsh config directories.

| Path                                                 | Purpose                                      |
| ---------------------------------------------------- | -------------------------------------------- |
| `~/.codex/AGENTS.md`                                 | Codex agent instructions                     |
| `~/.codex/config.toml`                               | Managed Codex config                         |
| `~/.codex/hooks.json`                                | Codex hook wiring                            |
| `~/.codex/hooks/`                                    | Codex hook adapters                          |
| `~/.codex/rules/default.rules`                       | Codex command permissions                    |
| `~/.codex/skills/`                                   | Rendered Codex skills                        |
| `~/.claude/CLAUDE.md`                                | Claude Code agent instructions               |
| `~/.claude/settings.json`                            | Claude Code settings                         |
| `~/.claude/hooks/`                                   | Claude Code hooks and policy guards          |
| `~/.claude/skills/`                                  | Rendered Claude Code skills                  |
| `~/.claude/skills/agent-harness/`                    | Claude Code plugin with the MCP servers      |
| `~/.claude/statusline/`                              | Claude Code status line command              |
| `~/.devin/hooks/`                                    | Devin hook adapters                          |
| `~/.config/devin/config.json`                        | Devin hooks and command permissions          |
| `~/.config/devin/mcp_config.json`                    | Devin MCP servers, synced into the real file |
| `~/.hermes/hooks.json`                               | Hermes Agent hook manifest                   |
| `~/.hermes/hooks/`                                   | Hermes Agent hook adapters                   |
| `~/.hermes/plugins/agent-harness-hooks/`             | Hermes Agent plugin that runs the manifest   |
| `~/.hermes/managed-config.yaml`                      | Hermes Agent keys to sync into `config.yaml` |
| `~/.pi/agent/AGENTS.md`                              | Shared agent instructions                    |
| `~/.pi/agent/skills/`                                | Skills with Claude-compatible frontmatter    |
| `~/.pi/agent/settings.json`                          | Managed model defaults merged with user keys |
| `~/.pi/agent/hooks.json`                             | pi hook manifest                             |
| `~/.pi/agent/extensions/hook_bridge.ts`              | pi extension that runs the manifest          |
| `~/.pi/hooks/`                                       | pi hook adapters                             |
| `~/.dsh/AGENTS.md`                                   | dsh agent instructions                       |
| `~/.dsh/hooks.json`                                  | dsh hook manifest                            |
| `~/.dsh/hooks/`                                      | dsh hook adapters                            |
| `~/.dsh/bridge/`                                     | dsh bridge bundle that runs the manifest     |
| `~/.dsh/skills/`                                     | Skills copied from the Codex rendering       |
| `~/.gemini/config/mcp_config.json`                   | Antigravity MCP servers, synced likewise     |

agent-harness gives Pi no OS-level isolation; run Pi inside a sandbox of your own when it needs
one ([ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)). Every install
removes the srt extension files earlier releases put in `~/.pi/agent/extensions/`
(`bash_sandbox.ts`, `sandbox_policy.json`, `sandbox/file_operations.mjs`), so keep an extension of
your own under another name. Hooks
are a policy layer only: as with Claude, a hook failure lets the call proceed.

Installation replaces the managed hook and skill directories and rewrites Claude Code's managed
settings. Codex config synchronization replaces only these managed top-level keys and preserves
other Codex-owned or user-owned state such as project trust and marketplace data:

```text
model, model_reasoning_effort, personality, approval_policy, sandbox_mode,
approvals_reviewer, cli_auth_credentials_store, notice, tui, plugins, features,
default_permissions, permissions
```

It also writes `analytics`, `otel`, `feedback`, and `check_for_update_on_startup` when the source
sets them, and never removes them when the source does not.

It also writes every key under the source's `shell_environment_policy.set` and removes keys a
previous sync wrote that the source dropped, tracked in `.agent-harness-managed.json` next to the
target. Keys Codex or the user added there stay. Without a sidecar, only `GH_CONFIG_DIR` is removed
when the source lacks it. `mcp_servers` entries are tracked the same way, so servers that Codex or
the user added stay.

Generated Claude settings keep every `permissions.allow`, `permissions.ask`, and
`permissions.deny` entry declared by the source `claude/settings.json`, followed by the generated
`Bash(...)`, `Read(...)`, and `Edit(...)` entries. Each rule is listed once, and a rule declared in
more than one list is kept only in its most restrictive list (`deny` over `ask` over `allow`), so
the merge never widens a permission. `defaultMode` and other permission keys pass through
unchanged. Claude settings synchronization replaces the whole generated `permissions` key, so
approvals Claude Code saves interactively into `~/.claude/settings.json` do not survive the next
sync; add persistent rules to `command_permissions.json` or the source `claude/settings.json`
instead.

Devin config synchronization replaces the managed top-level keys and, inside `permissions`,
replaces only `Exec(...)` entries. Non-Exec entries such as `Read(...)`, `Write(...)`, `Fetch(...)`,
and `mcp__*` rules stay user-owned. Approvals saved interactively as `Exec(...)` are replaced on the
next sync; add persistent command rules to `command_permissions.json` instead.

`install` does not edit `~/.hermes/config.yaml`. `sync-config --provider hermes` merges
`~/.hermes/managed-config.yaml` into it: generated `plugins.enabled` entries are appended once,
`mcp_servers` entries are tracked as for Codex, and all other Hermes Agent state is preserved. The Home Manager module runs this sync on activation.

`sync-config --provider dsh` registers the bridge bundle in a user-owned dsh profile's
`~/.dsh/profiles/<profile>/package.json`, the Home Manager option `dsh.profile` (default `web`): it
owns the
`dependencies` entries and adds the bundle to `dsh.profile.bundles` once, and it skips a profile
that dsh has not created yet. The bridge design is recorded in
[ADR-0025](docs/adr/0025_embed_the_dsh_hook_bridge.md).

`verify` checks the required paths and runtime commands declared by the resolved source manifest. It
does not compare installed file contents with the selected source.

## Usage

Most users only need:

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

For a project-local installation, set both the output prefix and runtime root. `--prefix` controls
where files are written; `--runtime-root` controls where generated hook commands expect those files
at runtime. The runtime root must be absolute.

```bash
agent-harness install \
  --source /absolute/path/to/agents \
  --prefix "$PWD" \
  --runtime-root "$PWD"
agent-harness verify --source /absolute/path/to/agents --prefix "$PWD"
```

The default runtime root remains the user's home directory, so existing user-level installs do not
need the additional option. Relocated hook commands expose the selected directory as
`AGENT_HARNESS_ROOT`; source-owned adapters that invoke sibling harness assets should resolve them
from `${AGENT_HARNESS_ROOT:-$HOME}`.

Inspect the built-in components managed by the resolved harness source:

```bash
agent-harness list
agent-harness list skills
agent-harness list hooks
agent-harness list skills --source ./agents
agent-harness list hooks --provider codex --source ./agents
```

The CLI also exposes lower-level generation commands for inspecting or composing individual outputs:

| Command                          | Output                                             |
| -------------------------------- | -------------------------------------------------- |
| `generate-hooks --provider <id>` | Provider hook JSON or config fragment              |
| `sync-config --provider <id>`    | Sync managed configuration into existing state     |
| `sync-config --provider <id> --file mcp-servers` | Sync the MCP servers into Devin's or Antigravity's own file |
| `generate-claude-settings`       | Complete Claude Code settings JSON                 |
| `generate-codex-config-source`   | Complete managed Codex config source               |
| `generate-codex-config-fragment` | Guarded-filesystem TOML fragment                   |
| `generate-codex-rules`           | Codex execpolicy rules                             |
| `generate-command-permissions`   | Shared runtime command permissions                 |
| `generate-forbidden-commands`    | Global precise forbidden-command regex rules       |
| `generate-hook-bundle`           | Isolated, versioned external hook bundle           |
| `generate-skills --provider <id>` | A provider's skills directory, built-in and external skills |

`generate-hooks` supports `claude`, `codex`, `devin`, `hermes`, `pi`, and `dsh`.
`sync-config` accepts the same provider IDs. For pi, it merges only
`defaultProvider`, `defaultModel`, and `defaultThinkingLevel` from the generated settings,
preserving user-owned keys. Optional source defaults belong in `pi/settings.json`.
Pi skills use the Claude rendering, including `disable-model-invocation`, and install under
`~/.pi/agent/skills/`; shared instructions install as `~/.pi/agent/AGENTS.md`.

Pi hook decisions use deny > ask > allow precedence. An ask decision prompts for confirmation
with the tool input when a UI is available; only "Yes" permits execution. Without a UI, asks block.
The old `generate-<provider>-hooks`, `sync-claude-settings`, and
`sync-{codex,devin,hermes,dsh}-config` commands still work as deprecated aliases hidden from help.

```bash
agent-harness generate-skills \
  --provider codex \
  --extra-skill external-tool=/path/to/external-tool \
  --output "$HOME/.codex/skills"

agent-harness generate-skills \
  --provider claude \
  --output "$HOME/.claude/skills"

agent-harness generate-hook-bundle \
  --spec /path/to/hook-bundle-spec.json \
  --output /path/to/hook-bundle

agent-harness generate-hooks --provider codex \
  --output "$HOME/.codex/hooks.json"

agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"

agent-harness generate-command-permissions \
  --output "$HOME/.claude/hooks/rules/command_permissions.json"

agent-harness generate-forbidden-commands \
  --output "$HOME/.claude/hooks/rules/forbidden_commands.json"

agent-harness generate-codex-config-source \
  --output /tmp/codex-config-source.toml

agent-harness sync-config --provider codex \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"

agent-harness generate-claude-settings \
  --output "$HOME/.claude/settings.json"
```

A hook bundle spec lists absolute installer executables and their arguments. Installers run in an
isolated temporary home, and only the supported provider hook artifacts are captured. Optional
command replacements let a build-time executable path become a stable runtime path:

```json
{
  "version": 1,
  "installers": [
    {
      "executable": "/nix/store/example/bin/example-hook",
      "arguments": ["install", "--target", "claude,codex"]
    }
  ],
  "command_replacements": [
    {
      "from": "/nix/store/example/bin/example-hook",
      "to": "/opt/homebrew/bin/example-hook"
    }
  ]
}
```

## Customization

For Nix-managed customization, prefer a complete flat source tree or the direct Home Manager
options above. Render a custom tree with
`agent-harness install --source <path> --prefix "$HOME"`.

| Goal                                       | Edit                                                   |
| ------------------------------------------ | ------------------------------------------------------ |
| Declare runtime command requirements       | `agents/manifest.json`                                 |
| Change shared agent instructions           | `agents/AGENTS.md`                                     |
| Add or edit a skill                        | `agents/skills/<skill>/SKILL.md`                       |
| Change provider-specific skill metadata    | `agents/skill_rendering.json`                          |
| Change shared command permissions          | `agents/command_permissions.json`                      |
| Change precise global command forms        | `agents/hooks/rules/{allowed,forbidden}_commands.json` |
| Add or change hook wiring                  | `agents/hooks.json`                                    |
| Add or change Claude hooks                 | `agents/hooks/*.sh`                                    |
| Add or change Codex hook adapters          | `agents/codex/hooks/*.sh`                              |
| Add or change Devin, Hermes, pi, dsh hooks | `agents/{devin,hermes,pi,dsh}/hooks/`                  |
| Change secret path or commit policy        | `agents/hooks/rules/secret_{path,commit}_policy.json`  |
| Change pi model defaults                   | `agents/pi/settings.json`                              |
| Change Codex base config                   | `agents/codex/config.toml`                             |
| Change Claude base settings                | `agents/claude/settings.json`                          |

Command permissions have two layers. `command_permissions.json` at the profile root is the
provider-neutral source of shared token prefixes. Each rule has a `decision` (`allow`, `ask`, or
`deny`), a `prefix`, and a required justification:

```json
{
  "version": 1,
  "rules": [
    {
      "decision": "ask",
      "prefix": ["git", "push"],
      "justification": "Publishing changes requires user confirmation."
    }
  ]
}
```

The generator maps these rules to Claude Code's `allow`, `ask`, and `deny` Bash permissions and to
Codex's `allow`, `prompt`, and `forbidden` execpolicy decisions, and to Devin's `Exec(...)`
`allow`, `ask`, and `deny` permissions. The `allowed_commands.json` and `forbidden_commands.json`
files under `hooks/rules/` contain POSIX extended regular expressions for precise global command
forms:

```json
{
  "version": 1,
  "rules": [
    {
      "programs": ["uv"],
      "patterns": ["^uv run --frozen example$"],
      "exceptions": ["^uv run --frozen example --unsafe$"],
      "justification": "Allow the repository's validated example task."
    }
  ]
}
```

`programs` and `exceptions` are optional. agent-harness validates these files and installs them
next to the hooks; enforcing them at runtime, including any project-local rule files, is the job
of the source's hook scripts. The built-in `minimal` source ships no such hooks.

`hooks/rules/secret_path_policy.json` and `hooks/rules/secret_commit_policy.json` are required as
well. A secret path rule that denies reads also becomes a Codex `[permissions.guarded.filesystem]`
`"none"` entry; see
[docs/DESIGN.md](docs/DESIGN.md) for the pattern rules.

Use the installed binary with a local source tree:

```bash
agent-harness install --source "$HOME/dotfiles/agents" --prefix "$HOME"
```

For repeated local rendering:

```bash
export AGENT_HARNESS_SOURCE="$HOME/dotfiles/agents"
agent-harness install --prefix "$HOME"
```

## Source Resolution

When `--source` is omitted, `agent-harness` resolves assets in this order:

1. explicit `--source`
2. `AGENT_HARNESS_SOURCE`
3. `share/agent-harness` below the binary directory
4. `share/agent-harness` below the binary installation prefix
5. current directory when it is an `agent-harness` source tree
6. embedded packaged assets

After resolving the source root, the CLI selects `profiles/<profile>` when present. Otherwise, it
treats the root as a complete flat profile. It must contain `manifest.json`, `AGENTS.md`,
`command_permissions.json`, `hooks.json`, `skill_rendering.json`, `claude/settings.json`,
`codex/config.toml`, and `hooks/rules/{allowed_commands,forbidden_commands,secret_commit_policy,secret_path_policy}.json`;
`skills/` and the other providers' hook directories are optional. This lets a dotfiles repository expose `agents/` directly while
the same binary continues to work from release tarballs, Nix builds, Cargo installs, and local
checkouts.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development environment, local checkout commands,
quality gates, and release process.
