# agent-harness

Shared AI agent harness files and installer for Codex and Claude Code.

`agent-harness` keeps reusable agent instructions, skills, hooks, command policies, and provider configuration in one source tree, then renders the provider-specific files needed by Codex and Claude Code. It is meant for keeping local machines, servers, and development containers on the same agent policy without hand-editing each tool's config directory.

## Why

- **One source of truth**: maintain shared agent instructions, skills, hooks, and command policy in this repository.
- **Provider-specific rendering**: render the same skill sources for Codex and Claude Code with provider-specific frontmatter and metadata.
- **Safer agent operation**: install guards for secret access, dangerous git commands, harness file edits, and command policy violations.
- **Managed config sync**: update managed Codex config keys while preserving tool-owned and user-owned state.
- **Portable install**: use a release installer, release tarball, Cargo, Nix, Home Manager, or a local source checkout.

## Quick Install

```bash
curl -fsSL https://github.com/furedea/agent-harness/releases/latest/download/install.sh | bash
agent-harness verify --prefix "$HOME"
```

The installer places the release archive under `$HOME/.local/agent-harness`, links the binary to `$HOME/.local/bin/agent-harness`, then runs `agent-harness install --prefix "$HOME"`.

Make sure `$HOME/.local/bin` is on `PATH`.

## Other Installation Methods

### Release Tarball

Use this on servers where you want the binary release but do not want to pipe an installer into Bash.

```bash
mkdir -p "$HOME/.local/bin" "$HOME/.local/agent-harness"

curl -fsSL \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.gz \
  | tar -xz -C "$HOME/.local/agent-harness" --strip-components=1

ln -sf "$HOME/.local/agent-harness/agent-harness" "$HOME/.local/bin/agent-harness"

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Cargo

Use this when you already have a Rust toolchain and want to build from the repository.

```bash
cargo install --git https://github.com/furedea/agent-harness agent-harness

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Nix

Use this when you want an ad hoc install from the flake package.

```bash
nix run github:furedea/agent-harness -- install --prefix "$HOME"
nix run github:furedea/agent-harness -- verify --prefix "$HOME"
```

### Home Manager

Use the Home Manager module when your agent config is managed by Nix. The module is exposed as `homeManagerModules.default`.

```nix
{
  inputs.agent-harness.url = "github:furedea/agent-harness";

  outputs =
    { agent-harness, ... }:
    {
      homeConfigurations.example = home-manager.lib.homeManagerConfiguration {
        modules = [
          agent-harness.homeManagerModules.default
        ];
      };
    };
}
```

## Optional Integrity Check

Release archives are published with a `SHA256SUMS` file.

```bash
curl -fsSL -O https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.gz
curl -fsSL -O https://github.com/furedea/agent-harness/releases/latest/download/install.sh
curl -fsSL -O https://github.com/furedea/agent-harness/releases/latest/download/SHA256SUMS
sha256sum -c SHA256SUMS
```

## What Gets Installed

`agent-harness install --prefix "$HOME"` writes the rendered harness into Codex and Claude Code config directories.

| Path                           | Purpose                             |
| ------------------------------ | ----------------------------------- |
| `~/.codex/AGENTS.md`           | Codex agent instructions            |
| `~/.codex/config.toml`         | Managed Codex config                |
| `~/.codex/hooks.json`          | Codex hook wiring                   |
| `~/.codex/hooks/`              | Codex hook adapters                 |
| `~/.codex/rules/default.rules` | Codex command policy                |
| `~/.codex/skills/`             | Rendered Codex skills               |
| `~/.claude/CLAUDE.md`          | Claude Code agent instructions      |
| `~/.claude/settings.json`      | Claude Code settings                |
| `~/.claude/hooks/`             | Claude Code hooks and policy guards |
| `~/.claude/skills/`            | Rendered Claude Code skills         |
| `~/.claude/statusline/`        | Claude Code status line command     |

`verify` checks that the required installed files exist.

## Usage

Most users only need:

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

The CLI also exposes lower-level generation commands for inspecting or composing individual outputs.

```bash
agent-harness generate-skills \
  --provider codex \
  --output "$HOME/.codex/skills"

agent-harness generate-skills \
  --provider claude \
  --output "$HOME/.claude/skills"

agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"

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

## Customization

Edit the source files, then run `agent-harness install --source <path> --prefix "$HOME"` or set `AGENT_HARNESS_SOURCE`.

| Goal | Edit |
| --- | --- |
| Change shared agent instructions | `agents/AGENTS.md` |
| Add or edit a skill | `agents/skills/<name>/SKILL.md` |
| Change provider-specific skill metadata | `agents/skill_rendering.json` |
| Allow or forbid shell commands | `agents/command_policy.json` |
| Add or change hook wiring | `agents/hooks.json` |
| Add or change Claude hooks | `agents/hooks/*.sh` |
| Add or change Codex hook adapters | `codex/hooks/*.sh` |
| Change Codex base config | `codex/config.toml` |
| Change Claude base settings | `claude/settings.base.json` |
| Add related-test mappings | `agents/hooks/rules/related_test_extensions.json` |
| Add secret detection patterns | `agents/hooks/rules/secret_content_patterns.json` |

### Local Source Checkout

```bash
git clone https://github.com/furedea/agent-harness
cd agent-harness

cargo run -- install --source "$PWD" --prefix "$HOME"
cargo run -- verify --prefix "$HOME"
```

Or use the installed binary with a local source tree:

```bash
agent-harness install --source /path/to/agent-harness --prefix "$HOME"
```

For repeated local rendering:

```bash
export AGENT_HARNESS_SOURCE=/path/to/agent-harness
agent-harness install --prefix "$HOME"
```

## Source Resolution

When `--source` is omitted, `agent-harness` resolves assets in this order:

1. explicit `--source`
2. `AGENT_HARNESS_SOURCE`
3. assets next to the installed binary
4. assets in a Nix-style `$prefix/share/agent-harness`
5. current directory when it is an `agent-harness` source tree
6. embedded packaged assets

This lets the same binary work from release tarballs, Nix builds, Cargo installs, and local checkouts.

## Development

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
actionlint .github/workflows/*.yml
bats --print-output-on-failure --recursive tests
```

## Release

Releases are managed by Release Please. Merging the release PR updates `Cargo.toml`, `Cargo.lock`, `.release-please-manifest.json`, and `CHANGELOG.md`, then publishes a GitHub Release with a Linux musl tarball and installer script.

If the release assets need to be rebuilt for an existing tag, run the `Release Please` workflow manually with the tag name, for example:

```text
agent-harness-v0.2.0
```
