# agent-harness

**Share hooks—not just skills—across Codex and Claude Code.**

`agent-harness` is a Nix-native composition layer for hooks, skills, agent instructions, command
permissions, and provider settings. Define one environment, then render the native files expected
by both Codex and Claude Code.

The neutral default stays small. An opt-in `furedea` profile provides the repository author's full
working environment, including secret protection, command guards, auditing, formatting, related
tests, and reusable skills. Home Manager users can start from either profile and inject local or
Nix-built sources without maintaining a separate profile repository.

```text
AGENTS.md and command permissions
Local or command-generated skills
Local or command-generated hook bundles
Provider settings
              |
              v
        agent-harness
        |-- Claude Code: settings, hooks, and skills
        `-- Codex: config, hooks, adapters, rules, and skills
```

## Why

- **Shared hooks**: reuse hook behavior across Codex and Claude Code, including tools whose CLI
  normally writes directly into each provider's config directory.
- **One set of guardrails**: generate Claude Bash permissions and Codex execpolicy from one neutral
  `allow` / `ask` / `deny` source.
- **Composable extensions**: combine built-in behavior with external skills and hook bundles.
- **CLI-to-Nix bridge**: turn command output into a skill, or capture supported files created by
  hook installers inside an isolated build home.
- **Provider-native output**: render the adapters, hook wiring, permissions, rules, and
  configuration expected by each agent.
- **Safe installation**: update managed configuration while preserving tool-owned and user-owned
  state.

## Profiles

| Profile   | Contents                                                                        |
| --------- | ------------------------------------------------------------------------------- |
| `minimal` | Neutral `AGENTS.md`; empty skill, hook, and permission sets; no model defaults. |
| `furedea` | 13 personal skills plus safety, audit, lint, test, and macOS UX hooks.          |

The `furedea` skills are `adr`, `bash-style`, `gha-style`, `git-commit-split`, `git-workflow`,
`github-ci-init`, `marp-style`, `nix-dev-init`, `nix-dotfiles`, `python-style`, `rust-style`,
`skill-auditor`, and `tsdd`. Its hooks enforce command and secret rules, protect generated harness
files, record audit events, format edited files, run related tests, send macOS notifications, and
adapt the shared behavior to Codex. The `minimal` profile keeps only the required neutral files, so
all of those opinions must be added explicitly.

Both the CLI and Home Manager module default to `minimal`. Select `furedea` to retain the behavior
that agent-harness shipped before profiles were introduced:

```bash
agent-harness --profile furedea install --prefix "$HOME"
```

Profiles are composition bases, not a requirement to keep all customization in this repository.
Home Manager can replace the shared files and add any number of skills and hook bundles directly.

## Compose with Nix

The Home Manager module accepts ordinary paths and derivations. A personal setup can therefore live
next to the rest of a user's Nix configuration:

```nix
programs.agent-harness = {
  enable = true;
  profile = "minimal";

  agentsMd = ./agents/AGENTS.md;
  commandPermissions = ./agents/command_permissions.json;

  skills = {
    team-workflows = ./agents/skills/team-workflows;
  };

  hooks = {
    notifications = ./agents/hook-bundles/notifications;
  };

  claude.settings.model = "claude-opus-4-1";
  codex.settings.model = "gpt-5.5";
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
Claude and Codex artifacts, and writes a versioned bundle. Each command is an argument list: the
first item is the executable and the remaining items are passed as arguments without a shell.
Executables must be absolute; Nix package paths naturally satisfy that requirement. Builds should
be deterministic and must not depend on interactive input or network access.

The hook builder captures `.claude/settings.json`, `.claude/hooks/`, `.codex/hooks.json`,
`.codex/hooks/`, `.codex/config.toml`, and top-level `.codex/*.sh` files. If generated hook commands
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

This installs the neutral `minimal` profile. Add `--profile furedea` before the subcommand to install
the repository author's full environment.

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

After installing the binary, `agent-harness install` is still required to render the Codex and
Claude Code configuration. `verify` only checks that the required installed paths exist.

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

The flake currently exposes a package only for `aarch64-darwin`.

```bash
nix run github:furedea/agent-harness -- install --prefix "$HOME"
nix run github:furedea/agent-harness -- verify --prefix "$HOME"
```

### Home Manager

Use the Home Manager module when your agent config is managed by Nix. The module is exposed as
`homeManagerModules.default` and currently targets `aarch64-darwin`.

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
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
bundles can be composed into both providers without copying them into this repository. Skill
directories must contain `SKILL.md`. Installed names may contain only lowercase ASCII letters,
digits, and hyphens, and may not shadow a built-in skill. Regular files are copied verbatim;
symlinks are ignored.

Each bundle must contain `hook_bundle.json` with `{"version":1}` and may provide Claude settings,
Codex hooks, Codex feature flags, and provider hook scripts. Scripts are installed below
`~/.claude/hooks/external/<name>/` or `~/.codex/hooks/external/<name>/`. Hook JSON is merged
structurally; external Codex configuration may add only `[features]` entries.

The complete module interface is:

| Option               | Default       | Meaning                                                       |
| -------------------- | ------------- | ------------------------------------------------------------- |
| `enable`             | `false`       | Enable the package and managed files.                         |
| `package`            | flake package | Select the `agent-harness` executable.                        |
| `source`             | this flake    | Advanced override for the profile source tree.                |
| `profile`            | `"minimal"`   | Select `minimal` or `furedea` as the composition base.        |
| `agentsMd`           | profile file  | Replace the shared `AGENTS.md` / `CLAUDE.md` source.          |
| `commandPermissions` | profile file  | Replace provider-neutral `allow` / `ask` / `deny` rules.      |
| `skills`             | `{}`          | Add skill directories or derivations by installed name.       |
| `hooks`              | `{}`          | Add versioned hook bundle directories or derivations by name. |
| `claude.enable`      | `true`        | Install Claude Code files.                                    |
| `claude.settings`    | `{}`          | Recursively merge settings over the selected Claude base.     |
| `codex.enable`       | `true`        | Install Codex files.                                          |
| `codex.settings`     | `{}`          | Recursively merge settings over the selected Codex base.      |

Generated hook wiring, Bash command permissions, and protected paths remain harness-managed after
the provider settings merge. This prevents a settings overlay from silently removing enforcement.
Set either provider's `enable` option to `false` to install only the other provider.

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

`agent-harness install --prefix "$HOME"` writes the rendered harness into Codex and Claude Code
config directories.

| Path                           | Purpose                             |
| ------------------------------ | ----------------------------------- |
| `~/.codex/AGENTS.md`           | Codex agent instructions            |
| `~/.codex/config.toml`         | Managed Codex config                |
| `~/.codex/hooks.json`          | Codex hook wiring                   |
| `~/.codex/hooks/`              | Codex hook adapters                 |
| `~/.codex/rules/default.rules` | Codex command permissions           |
| `~/.codex/skills/`             | Rendered Codex skills               |
| `~/.claude/CLAUDE.md`          | Claude Code agent instructions      |
| `~/.claude/settings.json`      | Claude Code settings                |
| `~/.claude/hooks/`             | Claude Code hooks and policy guards |
| `~/.claude/skills/`            | Rendered Claude Code skills         |
| `~/.claude/statusline/`        | Claude Code status line command     |

Installation replaces the managed hook and skill directories and rewrites Claude Code's managed
settings. Codex config synchronization replaces only these managed top-level keys and preserves
other Codex-owned or user-owned state such as project trust and marketplace data:

```text
model, model_reasoning_effort, personality, approval_policy, sandbox_mode,
approvals_reviewer, notice, tui, plugins, features, default_permissions,
permissions
```

`verify` checks for the required paths. It does not compare their contents with the selected source.

## Usage

Most users only need:

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

Inspect the built-in components managed by the resolved harness source:

```bash
agent-harness list
agent-harness list skills
agent-harness list hooks
agent-harness --profile furedea list skills
agent-harness --profile furedea list hooks --provider codex
```

The CLI also exposes lower-level generation commands for inspecting or composing individual outputs:

| Command                          | Output                                             |
| -------------------------------- | -------------------------------------------------- |
| `generate-claude-settings`       | Complete Claude Code settings JSON                 |
| `generate-claude-hooks`          | Claude Code hook JSON                              |
| `generate-codex-config-source`   | Complete managed Codex config source               |
| `generate-codex-config-fragment` | Guarded-filesystem TOML fragment                   |
| `generate-codex-hooks`           | Codex hook JSON                                    |
| `generate-codex-rules`           | Codex execpolicy rules                             |
| `generate-command-permissions`   | Shared runtime command permissions                 |
| `generate-forbidden-commands`    | Global precise forbidden-command regex rules       |
| `generate-hook-bundle`           | Isolated, versioned external hook bundle           |
| `generate-skills`                | Provider-specific built-in and external skill tree |
| `sync-codex-config`              | Managed-key merge into an existing Codex config    |

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

agent-harness generate-codex-hooks \
  --output "$HOME/.codex/hooks.json"

agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"

agent-harness generate-command-permissions \
  --output "$HOME/.claude/hooks/rules/command_permissions.json"

agent-harness generate-forbidden-commands \
  --output "$HOME/.claude/hooks/rules/forbidden_commands.json"

agent-harness generate-codex-config-source \
  --output /tmp/codex-config-source.toml

agent-harness sync-codex-config \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"

agent-harness generate-claude-settings \
  --output "$HOME/.claude/settings.json"
```

A hook bundle spec lists absolute installer executables and their arguments. Installers run in an
isolated temporary home, and only supported Claude and Codex hook artifacts are captured. Optional
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

For Nix-managed customization, prefer the direct Home Manager options above. To change a built-in
profile, edit its source files and run
`agent-harness --profile <minimal|furedea> install --source <path> --prefix "$HOME"`.

| Goal                                    | Edit                                                                   |
| --------------------------------------- | ---------------------------------------------------------------------- |
| Change shared agent instructions        | `profiles/<name>/agents/AGENTS.md`                                     |
| Add or edit a skill                     | `profiles/<name>/agents/skills/<skill>/SKILL.md`                       |
| Change provider-specific skill metadata | `profiles/<name>/agents/skill_rendering.json`                          |
| Change shared command permissions       | `profiles/<name>/agents/command_permissions.json`                      |
| Change precise global command forms     | `profiles/<name>/agents/hooks/rules/{allowed,forbidden}_commands.json` |
| Add or change hook wiring               | `profiles/<name>/agents/hooks.json`                                    |
| Add or change Claude hooks              | `profiles/<name>/agents/hooks/*.sh`                                    |
| Add or change Codex hook adapters       | `profiles/<name>/codex/hooks/*.sh`                                     |
| Change Codex base config                | `profiles/<name>/codex/config.toml`                                    |
| Change Claude base settings             | `profiles/<name>/claude/settings.base.json`                            |
| Add secret detection patterns           | `profiles/<name>/agents/hooks/rules/secret_content_patterns.json`      |

Command permissions have two layers. `agents/command_permissions.json` within a profile is the
provider-neutral source of shared token prefixes. Each rule has a `decision` (`allow`, `ask`, or
`deny`), a `prefix`, examples, and a justification:

```json
{
    "version": 1,
    "rules": [
        {
            "decision": "ask",
            "prefix": ["git", "push"],
            "examples": ["git push origin feature/example"],
            "justification": "Publishing changes requires user confirmation."
        }
    ]
}
```

The generator maps these rules to Claude Code's `allow`, `ask`, and `deny` Bash permissions and to
Codex's `allow`, `prompt`, and `forbidden` execpolicy decisions. The JSON files under
`agents/hooks/rules/` contain POSIX extended regular expressions for precise global command forms.

A repository may add precise rules without changing agent-harness by creating either of these
optional files:

```text
<git-root>/.agents/hooks/rules/allowed_commands.json
<git-root>/.agents/hooks/rules/forbidden_commands.json
```

Both files use this schema:

```json
{
    "version": 1,
    "rules": [
        {
            "patterns": ["^uv run --frozen example$"],
            "justification": "Allow the repository's validated example task."
        }
    ]
}
```

Project allow rules only approve precise forms within an allow prefix already declared in the
active profile's `agents/command_permissions.json`; they cannot introduce a new shared prefix.
Project forbidden rules may reject any matching command segment. A forbidden match takes
precedence over an allow match, and an invalid project rule file fails closed.

Use the installed binary with a local source tree:

```bash
agent-harness --profile furedea install --source /path/to/agent-harness --prefix "$HOME"
```

For repeated local rendering:

```bash
export AGENT_HARNESS_SOURCE=/path/to/agent-harness
agent-harness --profile furedea install --prefix "$HOME"
```

## Source Resolution

When `--source` is omitted, `agent-harness` resolves assets in this order:

1. explicit `--source`
2. `AGENT_HARNESS_SOURCE`
3. `share/agent-harness` below the binary directory
4. `share/agent-harness` below the binary installation prefix
5. current directory when it is an `agent-harness` source tree
6. embedded packaged assets

After resolving the source root, the CLI selects `profiles/<profile>`. A complete single-profile
source tree is also accepted for compatibility and advanced composition. This lets the same binary
work from release tarballs, Nix builds, Cargo installs, and local checkouts.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development environment, local checkout commands,
quality gates, and release process.
