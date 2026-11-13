# Design

<p align="center">
  <a href="DESIGN.md"><img alt="English" src="https://img.shields.io/badge/lang-English-0969da"></a>
  <a href="DESIGN_ja.md"><img alt="日本語" src="https://img.shields.io/badge/lang-%E6%97%A5%E6%9C%AC%E8%AA%9E-8c959f"></a>
</p>

This document explains why agent-harness is shaped the way it is. The [README](../README.md) covers
installation and usage, and each decision links to its [Architecture Decision Record](adr/).

## Problem

Each coding agent reads its own configuration format. Claude Code uses `settings.json` permission
rules, Codex uses execpolicy rules and `config.toml`, Devin uses `Exec(...)` permissions, and Hermes
Agent and pi each load hooks through their own manifest and plugin. Writing these by hand for every
agent breaks in predictable ways:

- **Guardrails drift.** A command denied in Claude Code stays allowed in Codex because only one of
    the two files was updated. Nothing reports the difference.
- **Agents can rewrite their own guards.** Protection is enforced in several layers: permission
    rules, sandbox write denials, filesystem guards, and an explanatory hook. A path missing from any
    one list is writable. Before v0.9.2, the Devin, Hermes, and pi hook files and the mutable provider
    configs were missing from the protected set ([#140]).
- **Hooks are duplicated per agent.** The same check must be re-implemented against each agent's
    hook protocol, and the copies diverge.
- **Provider-owned state collides with managed settings.** Claude Code and Codex write to their own
    config files (project trust, saved approvals). Overwriting those files loses that state, while
    letting a settings overlay win can silently remove enforcement. A related bug dropped Claude-only
    source rules such as `Bash(dangerouslyDisableSandbox:true)` during generation ([#140]).

## Core decisions

**1. Render every agent's files from one neutral source.**

- Decision: a flat source tree (`AGENTS.md`, `command_permissions.json`, `hooks.json`, skills, base
    provider settings) is the only place a user edits. Generated files are never edited by hand.
- Reason: one `allow` / `ask` / `deny` rule set maps to Claude Bash permissions, Codex execpolicy
    decisions, and Devin `Exec(...)` rules, so a change cannot reach one agent and miss another.
- ADRs: [0012], [0014], [0015].

**2. Derive protected paths from one Rust inventory and fail closed.**

- Decision: the protection module computes the protected paths from the same tables the installer
    uses, then emits Claude deny rules, sandbox `denyWrite`, Codex guards, and a JSON policy that the
    runtime hook reads.
- Reason: enforcement layers cannot disagree when they share one generated list. If the policy file
    is missing or invalid, the hook blocks instead of allowing.
- ADRs: [0008].

**3. Keep Codex CLI conformance out of the required checks.**

- Decision: required CI tests the repository's own generation logic. A separate workflow checks
    generated execpolicy rules against the real Codex CLI, whose locked version Renovate keeps
    current.
- Reason: repository-owned tests stay the stable source of truth, and breakage caused by a Codex
    release is reported on its own instead of failing unrelated checks.
- ADRs: [0001].

**4. Compose external skills and hook bundles at install time.**

- Decision: callers pass skill directories and versioned hook bundles. Hook bundles are captured by
    running a tool's installer in an isolated temporary home, then merged structurally.
- Reason: upstream tools such as Herdr own their hook protocols. agent-harness stays the single
    config writer without vendoring their files or adding tool-specific code.
- ADRs: [0004], [0006].

**5. Own only managed keys in mutable provider configs.**

- Decision: sync commands replace the parts of a config the harness owns and preserve everything
    else. Each provider declares what it owns in its spec (top-level keys, the list entries that
    match a prefix, a list to union, a mapping's entries, or keys tracked by a sidecar), and one
    engine applies any declaration to JSON, TOML, or YAML. Source
    permission rules are kept, deduplicated, and a rule in several lists stays only in the most
    restrictive one. The opt-in agent GitHub token is the one composition that overrides the source
    sandbox exclusions.
- Reason: agents keep writing their own state, but a settings overlay must never weaken or remove
    generated enforcement.
- ADRs: [0016], [0018], [0020], [0029].

**6. Separate provider declarations, functions, and I/O capabilities.**

- Decision: declare all six providers (Claude, Codex, Devin, Hermes, pi, and dsh) in
    `ProviderSpec`, with Data / Functions / Capabilities boundaries. Every field is one of five
    concepts most providers share: hook (`HookSpec`: scripts, registration, bridge files), skill,
    instructions, command rule, and config (`ConfigSpec`: path, how it is rendered, and the `ConfigSync` ownership declaration). Every
    provider has hooks; the other concepts are an `Option`. A provider difference is an enum
    variant, not a field only that provider has.
- Reason: shared paths, asset destinations, and `ConfigSync` declarations prevent parallel lists
    from drifting, and variants such as `HookRegistration` and `CommandRuleSpec` keep the
    differences visible and exhaustively matched. Lists follow registry order. Rust is the single
    source: `nix/providers.json` is generated from it by a test that fails when the committed file
    differs (`UPDATE_PROVIDERS_JSON=1` regenerates it). Home Manager links explicit managed files
    and subdirectories to coexist with user-owned state.
- ADRs: [0026], [0027], [0028], [0029].

```mermaid
flowchart LR
  subgraph D["Data (declarations only)"]
    SPEC["PROVIDERS (six ProviderSpec)"]
    CONCEPTS["five concepts: hooks, skills, instructions, command rules, config"]
    VARIANTS["variants: HookRegistration (InSettings, File), HookFormat (SettingsSection, WrappedEvents, BareEvents), CommandRuleSpec (InHookRegistration, RuleFiles, ExecpolicyFile), SkillSource (RenderFrontmatter, RenderWithInvocationPolicy, CopyRendered), ConfigLocation"]
    JSON["nix/providers.json (generated from PROVIDERS)"]
    KINDS["ConfigSync: ConfigFormat (Json, Toml, Yaml) and Ownership claims (Keys, Entries, ListUnion, MappingEntries, TrackedKeys)"]
  end
  subgraph F["Functions (computation, no I/O)"]
    RENDER["hook rendering: builds hook and config values"]
    RENDERER["ProviderRenderer: renders each provider's declared files"]
    PLAN["install_plan: what goes where"]
  end
  subgraph C["Capabilities (I/O)"]
    EXEC["shell::execute"]
    FS["shell::io (port)"]
    LOAD["shell::load (reader)"]
    SYNC["shell::sync"]
    ENGINE["config_sync::Merge (one engine)"]
  end
  SRC["Source: documents parsed once"]
  DISK[("disk")]
  SPEC -- "ProviderSpec" --> RENDER
  SPEC -- "ProviderSpec" --> RENDERER
  SPEC -- "ProviderSpec" --> PLAN
  SPEC --> CONCEPTS
  CONCEPTS --> VARIANTS
  CONCEPTS -- "config" --> KINDS
  SPEC -- "sourcegen test" --> JSON
  KINDS -- "ConfigSync declaration" --> ENGINE
  ENGINE -- "Synced text" --> SYNC
  ENGINE -- "Synced text" --> RENDERER
  RENDER -- "FileOp list" --> EXEC
  RENDERER -- "FileOp list" --> EXEC
  PLAN -- "FileOp list" --> EXEC
  SYNC -- "FileOp list" --> EXEC
  EXEC --> FS
  FS --> DISK
  DISK --> LOAD
  LOAD --> SRC
  SRC -- "Source" --> RENDER
  SRC -- "Source" --> RENDERER
```

- Only values cross layers: `ProviderSpec` flows from Data to Functions and Capabilities, a
    typed `Source` the loader built from the files computation reads flows from the loader to
    Functions, and `FileOp` lists flow from generation, rendering, planning, and sync to the executor,
    the only writer ([0028]).
- Three layers keep computation from writing, in one crate. `clippy.toml` bans std's write APIs and
    lifts the ban only in `shell::io`; the port is `pub(in crate::shell)`, so computation cannot reach
    it even through a wrapper; and `#![forbid(...)]` on `generation`, `provider::spec`,
    `provider::renderer`, `install_plan`, `file_op`, `source_model`, `layout`, `manifest`, `inventory`, and
    `render` stops them from allowing the ban again. A crate split was rejected: `std::fs` is callable
    from every crate, so the clippy ban stays necessary and the split would only replace visibility
    ([0028]). Tests are outside the lint.
- The same ban covers reads, so a result depends only on the inputs it was given and not on the disk
    at the moment it ran. `shell::load` reads the source tree, external bundles, and installed configs
    once into a typed `Source` (every document parsed and validated, an error naming its file; a
    directory that is only copied as a location and a file list), and computation reads only that.
    It bans `std::fs` reads, the `Path` probes (`exists`, `is_dir`, `read_dir`, `metadata`),
    `std::env`, and `std::process::Command`. The modules that are the program's imperative edge (`cli`,
    `source`, `runtime`, `github_app`, `shell::load`, `shell::hook_bundle`, and the build script) lift
    the ban for themselves with `#![expect(...)]` and a reason, which fails to compile once it stops
    being needed ([0028]).
- Only secret files (GitHub App tokens) are written `0600`; other files use the process's default
    mode, copies keep their source's permission bits, and replacing an existing file keeps its bits.
- Claude and Codex are declared in the registry like the other four providers; the registry declares
    what each owns, and provider functions render the content ([0027]).

## Alternatives considered

| Question                                              | Rejected alternative                                                              | Why it was rejected                                                                                                   |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Why not hand-written allow lists per agent?           | Separate provider lists, Bash path patterns in hooks                              | Lists drift between the hard boundary and the hook ([0008]).                                                          |
| Why not pin Codex CLI?                                | Pinning Codex in the dev shell, or adding it to required CI                       | Repository tests stay the source of truth, and Codex breakage is reported separately ([0001]).                        |
| Why not vendor Herdr's hooks?                         | Vendored copies (ADR-0002), then a Herdr-specific generator (ADR-0003)            | Vendored files went stale, and each new tool needed its own integration ([0006]).                                     |
| Why not replace or deep-merge `settings.json`?        | Immutable symlink, recursive JSON merge, materializing all assets                 | Claude Code also writes the file; owning top-level keys keeps its state without weakening guardrails ([0016]).        |
| Why not ship a personal profile?                      | An author-named built-in profile, or a mandatory profile repository               | Personal policy changes would force product releases ([0015]).                                                        |
| Why not put Claude-only rules in the neutral file?    | Provider syntax in `command_permissions.json`, stripping only prefix-shaped rules | The source Claude settings become the authoritative home for Claude-only rules ([0018]).                              |
| Why not infer project-local installs from `--prefix`? | Guessing the runtime scope from the output path                                   | Hooks assumed `$HOME` even when files were written elsewhere; an explicit root is self-contained ([0017]).            |
| Why not split computation into its own crate?         | A workspace with a no-I/O core crate                                              | `std::fs` is callable from any crate, so the clippy ban is still needed; the split only replaces visibility ([0028]). |

## Architecture

![agent-harness architecture](architecture.svg)

```mermaid
sequenceDiagram
  actor U as User / Home Manager
  participant A as agent-harness
  participant S as staging (temporary directory)
  participant H as install target (~/.claude, ~/.codex, ~/.devin, ...)
  U->>A: agent-harness install --source <dir>
  A->>A: read the source tree and choose providers from PROVIDERS
  A->>S: copy the existing Codex config so its own state is kept
  A->>S: write each provider's configs, hooks, permission rules and protected paths
  A->>S: place fixed files (assets) such as bridge scripts
  A->>A: decide the list of managed files and directories
  loop each managed path
    A->>S: read the finished file
    S-->>A: file contents
    A->>H: replace the same path
  end
  A-->>U: done
```

- The staging directory is the assembly area, so a failure while rendering leaves the install
    target untouched.
- The loop replaces each managed path by reading the finished file from staging.
- Files that are not managed, such as state an agent wrote itself, are left as they are.

`sync-config --provider <id>` commands merge managed keys into configs that agents also write.

| Module                                                                                    | Responsibility                                                                                                 |
| ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `cli`                                                                                     | Parses subcommands and dispatches to rendering, sync, list, and verify.                                        |
| `source`, `layout`, `manifest`                                                            | Resolve the source tree and validate its layout and required runtime commands.                                 |
| `profile`                                                                                 | Names the packaged built-in source (`minimal`).                                                                |
| `render`                                                                                  | Generates the staging tree and passes its publication plan to the executor.                                    |
| `provider/spec`, `provider/ownership`, `provider/registry`                                | Data: provider metadata, asset destinations, the `ConfigSync` ownership declaration, and the registry.         |
| `config_sync` (`codec`, `engine`, `installed`)                                            | Functions: format codecs, the one engine that applies declared ownership, and the parsed `Installed` config.   |
| `install_plan`                                                                            | Functions: plan publication of rendered staging files and embedded assets.                                     |
| `shell` (`io`, `load`, `executor`, `sync`, `hook_bundle`)                                 | Capabilities: the filesystem port, the `Source` loader, the `FileOp` executor, config sync, and hook bundles.    |
| `source_model`                                                                            | Functions: the typed `Source` (hook config, permissions, skills, settings, bundles) computation reads.         |
| `nix/providers.json`, `nix/home_manager_module.nix`                                       | Nix provider list generated from the registry, and the Home Manager options, links, and sync built from it.    |
| `generation/hooks`                                                                        | Render hook wiring.                                                                                            |
| `generation/bridges`, `provider/spec`, `install_plan`                                     | Verify bridge asset contracts; declare embedded bridge assets and destinations in specs and plan their writes. |
| `generation/command_permissions`                                                          | Maps neutral rules to Claude, Codex, and Devin permission syntax.                                              |
| `generation/claude_config`, `codex_config`, `dsh_config`                                  | Build provider configs and the dsh bundle registration.                                                        |
| `generation/protection`, `secret_path_policy`                                             | Compute protected paths and secret-file deny rules.                                                            |
| `generation/guarded_path`, `guarded_conflict`                                             | Compare Codex guarded filesystem entries and fail when a looser entry sits under a stricter one.               |
| `generation/skills`, `external_hooks`, `hook_bundle`                                      | Render skills and compose external bundles.                                                                    |
| `inventory`                                                                               | Lists the skills and hooks a source provides.                                                                  |
| `runtime_root`                                                                            | Relocates hook commands for project-local installs.                                                            |
| `github_app`                                                                              | Mints GitHub App installation tokens for the agent token option ([0021]).                                      |
| `runtime`                                                                                 | Checks that the manifest's runtime commands are on `PATH`.                                                     |

### Pi srt policy

The secret path policy reaches Pi as `filesystem.denyRead` entries of the srt policy, under the same
rule as Codex: only a rule that denies reads is rendered, and write-only rules stay with the hooks
because srt's restrictions apply to every process the agent starts. srt reads each entry as a glob
(`*`, `**`, `?`, `[...]`, no escapes, no braces, no negation) and expands `~` and `~/`; a relative
entry is anchored at the Git toplevel of the tool call's working directory (the working directory itself
outside a repository), not at the working directory, so a tool running in `/repo/src` still denies
`/repo/.env`. The toplevel is pinned per working directory like the common dir: the sandbox can rewrite `.git`
pointers, so a first resolution that later changes fails the call. A single-segment name (`.env`, `*.pem`) becomes `**/pattern` so
it matches at any depth as in Claude, and `//x` becomes `/x`. A relative pattern with a `/` other than a trailing one
that does not start with `**/` (`secrets/**`, `config/x.json`, `./secrets/**`) is rejected when the secret path policy is
loaded: gitignore and Codex anchor it at the repository root, srt matches it at any depth, and Claude anchors it at the working
directory, so one pattern would guard three different sets of files. Write `**/<pattern>` for any depth. Generation fails, naming the pattern, for
a leading `/`, `.` or `..` parts, backslashes, braces, a leading `!`, a negated character class (`[!a]`, `[^a]`), or a wildcard in the first
component of an absolute pattern (srt on Linux skips it). A `**/` pattern guards only beneath the
repository root, as `:workspace_roots` does for Codex.

### Codex guarded filesystem

The secret path policy reaches Codex as `[permissions.guarded.filesystem]` entries: a rule that
denies reads is `"none"`. A rule that only denies writes is not rendered for Codex at all. Codex's
filesystem permissions apply to every process the agent starts, while Claude's `Edit(...)` deny
applies only to the Edit tool, and the write-only rules guard harness state that harness scripts
write themselves (`verification_session.py` writes under `**/agent-harness/{verification,
trusted-owners,audit}/**`). Denying those writes in Codex's sandbox would break the scripts; hooks
keep enforcing them. This matches the Claude Code sandbox, which denies reading such paths but
allows writing to `~/.local/state/agent-harness`.

Codex applies the more specific entry, so a looser entry under a stricter one (a protected
`~/.codex/hooks/adapt.sh = "read"` under `~/.codex/** = "none"`) reopens a denied path. The harness
never rewrites guarded entries to guess which side was meant. It compares the generated entries
with each other and with the source `codex/config.toml` and fails, naming both entries, when one
covers a looser one. A path resolves through the nearest entry that covers it, not every ancestor,
so a looser intermediate entry (`~ = "write"` under `:root = "deny"`) is the exception for the
entries below it. Only denials may be patterns; the looser entry is always a concrete path, so
two globs are never compared with each other and a glob looser than another glob is not detected.
A rule that denies reads with a brace pattern fails generation, because Codex does not expand
braces. Paths are compared by root: `~`, absolute paths, UNC paths and the workspace are separate,
except that a project-local install (`--runtime-root` is the project) matches an absolute path
inside the project against `:workspace_roots` too. Windows-form paths (a drive letter, a UNC
prefix, or any path on a Windows host) take `\` as a separator and compare without regard to
case; elsewhere `\` is a filename character. Conflict detection is a safety net; hooks still enforce the secret policy at
runtime.

On platforms that expand unreadable globs before the sandbox starts, Codex caps `**` at
`glob_scan_max_depth` (5), so a secret nested deeper stays readable by a command that bypasses the
hooks. Hook-time enforcement remains the primary guard.

## Verification

Figures below were measured on v0.9.2.

| Layer               | Location                                                                                  | Count    |
| ------------------- | ----------------------------------------------------------------------------------------- | -------- |
| Unit tests          | `#[test]` in `src/`                                                                       | 77       |
| Integration tests   | `tests/*.rs`, running the built binary                                                    | 36       |
| Codex conformance   | `tests/codex/execpolicy.bats`                                                             | 3 cases  |
| Home Manager module | `tests/home_manager_module.nix`, `tests/home_manager_pure_eval.nix` via `nix flake check` | 2 checks |

The flake also builds the skill and hook-bundle builders as checks. The module reads every
evaluation-time file (`settings.json`, `config.toml`, `secret_path_policy.json`, `profiles/`) from
`source.tree`, falling back to `source` when it has no `tree`. The built `source` only supplies files
copied at build time, so evaluation never builds it: no import-from-derivation
([furedea/agent-harness#196](https://github.com/furedea/agent-harness/issues/196)). The shared
`nix-check` workflow runs `nix flake check --all-systems` with import-from-derivation disabled.
GitHub Actions runs three workflows:

- `ci`: clippy, rustfmt, `cargo test`, and Rust dependency audits, plus calls to the shared
    workflows in `furedea/reusable-workflows` (`nix-lint`, `nix-check`, `gha-lint`,
    `dependency-review`, `claude-review`, `codex-review`), gathered by an `all-green` job.
- `codex-conformance`: generated execpolicy rules against the locked Codex CLI, kept separate from
    `ci` by design ([0001]).
- `release`: versioning and changelog through the shared `release-pr` and `release-publish`
    workflows, with the cargo-dist build of the draft release tag in between.

CodeQL analysis is not a workflow file: it runs through GitHub's CodeQL default setup.

## Known limitations

- Release binaries are built only for `x86_64-unknown-linux-musl`. The Nix package, Home Manager
    module, and checks target `x86_64-linux` and `aarch64-darwin`.
- The Home Manager `profile` option accepts only `minimal`. Other setups pass a complete `source`.
- `verify` checks required paths and runtime commands. It does not compare installed contents with
    the source.
- A key removed from generated Claude settings is not removed from an existing `settings.json`
    ([0016]).
- Approvals that Claude Code or Devin save interactively are replaced on the next sync. Persistent
    rules belong in the source.
- Skills are rendered but not protected.
- The GitHub App token source talks only to github.com, not GitHub Enterprise Server ([0021]).

## Changelog highlights

- **v0.7.0**: removed the Herdr-specific integration, leaving the generic hook bundles added in
    v0.6.0 as the only way to compose external hooks ([0006]).
- **v0.8.0**: split personal configuration out of the product, adopted flat source trees, and renamed
    command policy to command permissions ([0012], [0014], [0015]).
- **v0.9.2**: derived protected paths from every rendered provider file and kept source Claude
    permission rules during generation ([#140], [0018]).

[#140]: https://github.com/furedea/agent-harness/pull/140
[0001]: adr/0001_separate_codex_conformance_from_required_ci.md
[0004]: adr/0004_inject_external_skills_at_install_time.md
[0006]: adr/0006_compose_external_hook_bundles.md
[0008]: adr/0008_generate_runtime_protection_policy.md
[0012]: adr/0012_name_provider_neutral_command_permissions.md
[0014]: adr/0014_use_flat_profile_sources.md
[0015]: adr/0015_separate_personal_agent_source.md
[0016]: adr/0016_materialize_mutable_claude_settings.md
[0017]: adr/0017_separate_output_and_runtime_roots.md
[0018]: adr/0018_preserve_source_claude_permission_rules.md
[0020]: adr/0020_keep_git_push_outside_the_claude_sandbox.md
[0021]: adr/0021_refresh_the_agent_token_from_a_github_app.md
[0026]: adr/0026_declare_providers_in_a_registry.md
[0027]: adr/0027_declare_claude_and_codex_in_the_provider_registry.md
[0028]: adr/0028_return_file_operations_as_values.md
[0029]: adr/0029_declare_config_ownership_and_sync_with_one_engine.md
