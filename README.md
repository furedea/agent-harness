# agent-harness

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/lang-English-0969da"></a>
  <a href="README_ja.md"><img alt="日本語" src="https://img.shields.io/badge/lang-%E6%97%A5%E6%9C%AC%E8%AA%9E-8c959f"></a>
</p>

**Share hooks—not just skills—across coding agents.**

`agent-harness` is a Nix-native composition layer for hooks, skills, agent instructions, command
permissions, and provider settings. Define one environment, then render the native files expected
by Claude Code, Codex, Devin, Hermes Agent, and pi.

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
        `-- pi: hook manifest and bridge extension
```

## Why

- **Shared hooks**: reuse hook behavior across Claude Code, Codex, Devin, Hermes Agent, and pi,
  including tools whose CLI normally writes directly into each provider's config directory.
- **One set of guardrails**: generate Claude Bash permissions, Codex execpolicy, and Devin Exec
  permissions from one neutral `allow` / `ask` / `deny` source.
- **Composable extensions**: combine built-in behavior with external skills and hook bundles.
- **CLI-to-Nix bridge**: turn command output into a skill, or capture supported files created by
  hook installers inside an isolated build home.
- **Provider-native output**: render the adapters, hook wiring, permissions, rules, and
  configuration expected by each agent.
- **State-aware config sync**: replace harness-owned keys while preserving tool-owned and
  user-owned state in the Claude Code, Codex, Devin, and Hermes Agent configs.

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
Claude and Codex artifacts, and writes a versioned bundle. Each command is an argument list: the
first item is the executable and the remaining items are passed as arguments without a shell.
Hook installer executables must be absolute; absolute Nix package paths are recommended for skill
commands as well. Builds should be deterministic and must not depend on interactive input or
network access.

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

### Agent GitHub token in the Claude sandbox

By default the user's GitHub token is readable only outside the sandbox, so the Claude source
settings list `gh *` in `sandbox.excludedCommands`. A call that chains gh with other commands then
runs sandboxed, fails authentication, and ends in an approval prompt. The opt-in `github.agentToken`
option gives Claude Code its own token that is readable inside the sandbox:

1. Create a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
   limited to the repositories agents may change, with Contents, Pull requests, and Issues set to
   read and write, Actions and Metadata set to read, and an expiry such as 90 days.
2. Store it in the agent config directory. `--insecure-storage` writes the token to `hosts.yml` in
   that directory instead of the system keyring, which the sandbox cannot reach and which already
   holds the user's own token:

    ```bash
    GH_CONFIG_DIR="$HOME/.config/agent-harness/gh" gh auth login --with-token --insecure-storage < token.txt
    ```

3. Enable the option and switch the Home Manager configuration:

    ```nix
    programs.agent-harness.github.agentToken.enable = true;
    # configDir defaults to "${config.home.homeDirectory}/.config/agent-harness/gh".
    ```

The rendered Claude settings then set `env.GH_CONFIG_DIR` to the absolute `configDir`, add
`github.com` and `api.github.com` to `sandbox.network.allowedDomains`, set
`sandbox.enableWeakerNetworkIsolation` so gh can verify TLS, and remove `gh *` from
`sandbox.excludedCommands` even when the source settings list it. That removal is the one exception
to the source settings being authoritative ([ADR-0020](docs/adr/0020_keep_git_push_outside_the_claude_sandbox.md)).
`git push *` and every other exclusion stay as the source lists them, so the source settings choose
where git runs (see [Agent commits](#agent-commits)).
Evaluation fails when `configDir` is relative or when a `Read(...)` denial, `sandbox.filesystem.denyRead`
entry, or secret path policy rule hides it. Denials of the user's own `~/.config/gh/hosts.yml` stay
in place. With the option disabled, the rendered output is unchanged.

The rendered Codex config sets `shell_environment_policy.set.GH_CONFIG_DIR` to the same `configDir`
and makes the `guarded` permissions profile the default (`default_permissions = "guarded"`, which
takes precedence over `sandbox_mode`). The profile reads everything (`":root" = "read"`) except
`~/.config/gh/hosts.yml`, writes the workspace (`:workspace_roots`), `:tmpdir`, `:slash_tmp`, and
the absolute and `~/` paths in Claude's `sandbox.filesystem.allowWrite`, and enables network. Codex
config synchronization manages only the `GH_CONFIG_DIR` key inside `shell_environment_policy`, so
other entries Codex or the user keep there survive.

Codex keeps the git directory of a linked worktree (`<repo>/.git/worktrees/<name>`) read-only unless
the profile names that exact path, so commits from a worktree still need approval in Codex; commits
in a main checkout run inside the sandbox
([ADR-0023](docs/adr/0023_default_codex_to_the_guarded_profile_with_the_agent_token.md)).

#### Agent commits

Commit signing works only outside the sandbox, so the source settings pick one of two setups for
agent git:

- **(a) Sign agent commits.** List `git add *`, `git status *`, `git commit *`, and `git push *` in
  `sandbox.excludedCommands` so a chained `git add x && git commit -m y && git push` runs outside
  the sandbox, where signing works. An excluded `git push` still inherits `GH_CONFIG_DIR` and
  authenticates with the agent token. Repository git hooks then also run outside the sandbox, so an
  agent can run unsandboxed code by editing a hook configuration such as `lefthook.yml`, a script a
  hook calls, or build settings that a hook's tool reads.
- **(b) Leave agent commits unsigned (recommended when hooks or the tools they run read repository
  files).** Disable signing for Claude Code only and exclude no git commands, so git, its hooks, and
  push all run inside the sandbox and push authenticates with the agent token, from either source.
  Commits that reach the default branch through squash merges are still signed, by GitHub
  ([ADR-0022](docs/adr/0022_leave_agent_commits_unsigned_inside_the_claude_sandbox.md)).

For (b), set `GIT_CONFIG_*` in the Claude source settings' `env`. It overrides git's config files
for Claude Code processes only, while the user's own commits stay signed (git 2.31 or later):

```json
{
  "env": {
    "GIT_CONFIG_COUNT": "2",
    "GIT_CONFIG_KEY_0": "commit.gpgsign",
    "GIT_CONFIG_VALUE_0": "false",
    "GIT_CONFIG_KEY_1": "tag.gpgsign",
    "GIT_CONFIG_VALUE_1": "false"
  }
}
```

Threat model:

- Anything running inside the sandbox can use the token within its permissions, including direct
  API calls; the token's fine-grained scope, not the command rules, is the safety boundary.
- gh runs inside the sandbox on purpose: outside it, gh could upload any readable local file, for
  example with `gh gist create`, `gh release upload`, or `gh api --input`. git push outside the
  sandbox in setup (a) can only send repository commits.
- git authenticates through `gh auth git-credential` (set up with `gh auth setup-git`), so remotes
  must use HTTPS. SSH remotes keep using the user's SSH keys outside this mechanism.
- Claude Code and Codex shell commands receive the token. Devin, Hermes, and pi still use the
  default behavior.

#### GitHub App token source

A personal access token has to be recreated when it expires. With `source = "app"`, a GitHub App
supplies the agent token instead: a per-user scheduled job checks the token at login and every 5
minutes, mints a one-hour installation access token when the current one has less than 20 minutes
left, and writes it to `hosts.yml` in `configDir` atomically with mode 0600. The job is a launchd agent on macOS, which also runs a missed interval after wake, and a
systemd user service with a persistent timer on Linux. Everything the option does to the Claude and
Codex settings is the same for both sources.

1. [Register a GitHub App](https://github.com/settings/apps/new) with webhooks disabled and these
   repository permissions: Contents, Pull requests, and Issues set to read and write; Actions and
   Metadata set to read. Do not grant Workflows: it would let sandboxed code rewrite workflows that
   run with repository secrets.
2. Install the App on the repositories agents may change.
3. Generate a private key and store it with mode 0600 outside `configDir`:

    ```bash
    install -m 0600 ~/Downloads/my-agent-app.*.private-key.pem ~/.config/agent-harness/github_app.pem
    ```

4. Configure the source and switch:

    ```nix
    programs.agent-harness.github.agentToken = {
      enable = true;
      source = "app";
      app = {
        id = 123456;
        privateKeyPath = "${config.home.homeDirectory}/.config/agent-harness/github_app.pem";
        # installationId = 7890123;  # Required only when the App has several installations.
      };
    };
    ```

The private key path must be absolute. The rendered Claude settings add `Read(//<path>)` to
`permissions.deny` and the path to `sandbox.filesystem.denyRead`, so neither Claude Code tools nor
sandboxed commands can read the key. The rendered Codex config denies the key in
`[permissions.guarded.filesystem]` as well. With `installationId` unset, the job uses the App's only
installation and fails when it has several.

The job records each refresh in `agent_harness_token.json` beside the token. If it stops, pushes
start failing within the hour, so check it with:

```bash
agent-harness github-app-token status --config-dir ~/.config/agent-harness/gh
```

The command prints the expiry and exits with status 1 when no token exists, the last refresh
failed, or the token has five minutes or less left. Job output goes to
`~/Library/Logs/agent-harness-github-app-token.log` on macOS and to the user journal on Linux.
`agent-harness github-app-token refresh --force` mints a token on demand with the same arguments as
the job; without `--force` it keeps a token that has 20 minutes or more left.

Additional threat model for the App source:

- The private key is a long-lived secret equivalent to an SSH key: anyone holding it can mint tokens
  for every installation. Keep it at mode 0600 and out of backups that others can read.
- Installation tokens expire within an hour, so a leaked token is short-lived.
- Pushes, pull requests, and comments appear as the App's bot account.
- A token with Pull requests write can approve pull requests. If branch protection ever requires
  approvals, do not let the bot count as an approver.

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
| `source`             | this flake    | Profile collection or complete flat profile source tree.      |
| `profile`            | `"minimal"`   | Select the packaged composition base.                         |
| `agentsMd`           | profile file  | Replace the shared `AGENTS.md` / `CLAUDE.md` source.          |
| `commandPermissions` | profile file  | Replace provider-neutral `allow` / `ask` / `deny` rules.      |
| `skills`             | `{}`          | Add skill directories or derivations by installed name.       |
| `hooks`              | `{}`          | Add versioned hook bundle directories or derivations by name. |
| `claude.enable`      | `true`        | Install Claude Code files.                                    |
| `claude.settings`    | `{}`          | Optionally override keys in the selected Claude base.         |
| `codex.enable`       | `true`        | Install Codex files.                                          |
| `codex.settings`     | `{}`          | Optionally override keys in the selected Codex base.          |
| `devin.enable`       | `true`        | Install Devin files.                                          |
| `hermes.enable`      | `true`        | Install Hermes Agent files.                                   |
| `pi.enable`          | `true`        | Install pi files.                                             |

Generated hook wiring, Bash command permissions, and protected paths remain harness-managed after
the provider settings merge. This prevents a settings overlay from silently removing enforcement.
Protected paths cover every file the harness renders or syncs for Claude Code, Codex, Devin, Hermes,
and pi, including the mutable provider configs, but not skills.
Set a provider's `enable` option to `false` to skip its files.

The Home Manager module materializes `~/.claude/settings.json` as a writable regular file because
Claude Code may update it. On activation, generated top-level keys replace the corresponding
existing values, while top-level keys absent from the generated settings are preserved. Other
Claude Code files remain Home Manager-managed symlinks. Activation also syncs the Codex, Devin, and
Hermes Agent configs as described in [What Gets Installed](#what-gets-installed).

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
Devin, Hermes Agent, and pi config directories.

| Path                                     | Purpose                                      |
| ---------------------------------------- | -------------------------------------------- |
| `~/.codex/AGENTS.md`                     | Codex agent instructions                     |
| `~/.codex/config.toml`                   | Managed Codex config                         |
| `~/.codex/hooks.json`                    | Codex hook wiring                            |
| `~/.codex/hooks/`                        | Codex hook adapters                          |
| `~/.codex/rules/default.rules`           | Codex command permissions                    |
| `~/.codex/skills/`                       | Rendered Codex skills                        |
| `~/.claude/CLAUDE.md`                    | Claude Code agent instructions               |
| `~/.claude/settings.json`                | Claude Code settings                         |
| `~/.claude/hooks/`                       | Claude Code hooks and policy guards          |
| `~/.claude/skills/`                      | Rendered Claude Code skills                  |
| `~/.claude/statusline/`                  | Claude Code status line command              |
| `~/.devin/hooks/`                        | Devin hook adapters                          |
| `~/.config/devin/config.json`            | Devin hooks and command permissions          |
| `~/.hermes/hooks.json`                   | Hermes Agent hook manifest                   |
| `~/.hermes/hooks/`                       | Hermes Agent hook adapters                   |
| `~/.hermes/plugins/agent-harness-hooks/` | Hermes Agent plugin that runs the manifest   |
| `~/.hermes/managed-config.yaml`          | Hermes Agent keys to sync into `config.yaml` |
| `~/.pi/agent/hooks.json`                 | pi hook manifest                             |
| `~/.pi/agent/extensions/hook_bridge.ts`  | pi extension that runs the manifest          |
| `~/.pi/hooks/`                           | pi hook adapters                             |

Installation replaces the managed hook and skill directories and rewrites Claude Code's managed
settings. Codex config synchronization replaces only these managed top-level keys and preserves
other Codex-owned or user-owned state such as project trust and marketplace data:

```text
model, model_reasoning_effort, personality, approval_policy, sandbox_mode,
approvals_reviewer, notice, tui, plugins, features, default_permissions,
permissions
```

It also manages the single key `shell_environment_policy.set.GH_CONFIG_DIR`, set or removed to
match the source, and leaves the rest of `shell_environment_policy` alone.

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

`install` does not edit `~/.hermes/config.yaml`. `sync-hermes-config` merges
`~/.hermes/managed-config.yaml` into it: generated `plugins.enabled` entries are appended once, and
all other Hermes Agent state is preserved. The Home Manager module runs this sync on activation.

`verify` checks the required paths and runtime commands declared by the resolved source manifest. It
does not compare installed file contents with the selected source.

## Usage

Most users only need:

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

For a project-local installation, set both the output prefix and runtime root. `--prefix` controls
where files are written; `--runtime-root` controls where generated hook commands and protection
rules expect those files at runtime. The runtime root must be absolute.

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
| `generate-claude-settings`       | Complete Claude Code settings JSON                 |
| `generate-claude-hooks`          | Claude Code hook JSON                              |
| `generate-codex-config-source`   | Complete managed Codex config source               |
| `generate-codex-config-fragment` | Guarded-filesystem TOML fragment                   |
| `generate-codex-hooks`           | Codex hook JSON                                    |
| `generate-codex-rules`           | Codex execpolicy rules                             |
| `generate-command-permissions`   | Shared runtime command permissions                 |
| `generate-devin-hooks`           | Devin config fragment (hooks and permissions)      |
| `generate-forbidden-commands`    | Global precise forbidden-command regex rules       |
| `generate-hermes-hooks`          | Hermes Agent hook manifest                         |
| `generate-hook-bundle`           | Isolated, versioned external hook bundle           |
| `generate-pi-hooks`              | pi hook manifest                                   |
| `generate-skills`                | Provider-specific built-in and external skill tree |
| `sync-claude-settings`           | Top-level merge into existing Claude settings      |
| `sync-codex-config`              | Managed-key merge into an existing Codex config    |
| `sync-devin-config`              | Managed merge into an existing Devin config        |
| `sync-hermes-config`             | Managed-key merge into an existing Hermes config   |

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

For Nix-managed customization, prefer a complete flat source tree or the direct Home Manager
options above. Render a custom tree with
`agent-harness install --source <path> --prefix "$HOME"`.

| Goal                                    | Edit                                                   |
| --------------------------------------- | ------------------------------------------------------ |
| Declare runtime command requirements    | `agents/manifest.json`                                 |
| Change shared agent instructions        | `agents/AGENTS.md`                                     |
| Add or edit a skill                     | `agents/skills/<skill>/SKILL.md`                       |
| Change provider-specific skill metadata | `agents/skill_rendering.json`                          |
| Change shared command permissions       | `agents/command_permissions.json`                      |
| Change precise global command forms     | `agents/hooks/rules/{allowed,forbidden}_commands.json` |
| Add or change hook wiring               | `agents/hooks.json`                                    |
| Add or change Claude hooks              | `agents/hooks/*.sh`                                    |
| Add or change Codex hook adapters       | `agents/codex/hooks/*.sh`                              |
| Add or change Devin, Hermes, pi hooks   | `agents/{devin,hermes,pi}/hooks/`                      |
| Change Codex base config                | `agents/codex/config.toml`                             |
| Change Claude base settings             | `agents/claude/settings.json`                          |

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
Codex's `allow`, `prompt`, and `forbidden` execpolicy decisions. The JSON files under `hooks/rules/`
contain POSIX extended regular expressions for precise global command forms.

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
active profile's `command_permissions.json`; they cannot introduce a new shared prefix.
Project forbidden rules may reject any matching command segment. A forbidden match takes
precedence over an allow match, and an invalid project rule file fails closed.

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
treats the root as a complete flat profile containing `manifest.json`, `AGENTS.md`, `hooks/`,
`skills/`, `claude/`, and `codex/`. This lets a dotfiles repository expose `agents/` directly while
the same binary continues to work from release tarballs, Nix builds, Cargo installs, and local
checkouts.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development environment, local checkout commands,
quality gates, and release process.
