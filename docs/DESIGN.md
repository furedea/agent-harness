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

- Decision: sync commands replace harness-owned keys and preserve everything else. Source
  permission rules are kept, deduplicated, and a rule in several lists stays only in the most
  restrictive one. The opt-in agent GitHub token is the one composition that overrides the source
  sandbox exclusions.
- Reason: agents keep writing their own state, but a settings overlay must never weaken or remove
  generated enforcement.
- ADRs: [0016], [0018], [0020].

**6. Separate provider declarations, functions, and I/O capabilities.**

- Decision: declare all six providers (Claude, Codex, Devin, Hermes, pi, and dsh) in
  `ProviderSpec`, with Data / Functions / Capabilities boundaries. Every field is one of five
  concepts most providers share: hook (`HookSpec`: scripts, registration, bridge files), skill,
  instructions, command rule, and config (`ConfigSpec`: path and `ConfigSync` kind). Every
  provider has hooks; the other concepts are an `Option`. A provider difference is an enum
  variant, not a field only that provider has.
- Reason: shared paths, asset destinations, and `ConfigSync` strategies prevent parallel lists
  from drifting, and variants such as `HookRegistration` and `CommandRuleSpec` keep the
  differences visible and exhaustively matched. Lists follow registry order. Rust is the single
  source: `nix/providers.json` is generated from it by a test that fails when the committed file
  differs (`UPDATE_PROVIDERS_JSON=1` regenerates it). Home Manager links explicit managed files
  and subdirectories to coexist with user-owned state.
- ADRs: [0026], [0027].

```mermaid
flowchart LR
  subgraph D["Data (declarations only)"]
    SPEC["PROVIDERS (six ProviderSpec)"]
    CONCEPTS["five concepts: hooks, skills, instructions, command rules, config"]
    VARIANTS["variants: HookRegistration (InSettings, File), CommandRuleSpec (DevinHookBlock, ClaudeRules, CodexExecpolicy), SkillSource"]
    JSON["nix/providers.json (generated from PROVIDERS)"]
    KINDS["ConfigSync kinds: MergeClaudeSettings, MergeToml, MergeTopLevelJson, MergeYaml, RegisterBundle"]
  end
  subgraph F["Functions (computation, no I/O)"]
    RENDER["hook rendering: builds hook and config values"]
    RENDERER["ProviderRenderer: renders each provider's declared files"]
    PLAN["install_plan: what goes where"]
  end
  subgraph C["Capabilities (I/O)"]
    EXEC["install_executor"]
    FS["FileSystem (trait)"]
    SYNC["config_sync"]
  end
  DISK[("disk")]
  SPEC -- "ProviderSpec" --> RENDER
  SPEC -- "ProviderSpec" --> RENDERER
  SPEC -- "ProviderSpec" --> PLAN
  SPEC --> CONCEPTS
  CONCEPTS --> VARIANTS
  CONCEPTS -- "config" --> KINDS
  SPEC -- "sourcegen test" --> JSON
  KINDS -- "ConfigSync kind" --> SYNC
  PLAN -- "FileOp list" --> EXEC
  EXEC --> FS
  FS --> DISK
  SYNC --> DISK
```

- Only values cross layers: `ProviderSpec` flows from Data to Functions and Capabilities, and the
  `FileOp` list flows from `install_plan` to the executor.
- `tests/provider_spec_layering.rs` checks that the spec, install planner, and hook and bridge
  rendering modules do not reference `fs_ops` or `std::fs`. It is a source-text check, not a
  transitive analysis: staging generation still writes through the existing generation path
  ([0026]).
- Claude and Codex are declared in the registry like the other four providers; the registry declares
  what each owns, and provider functions render the content ([0027]).

## Alternatives considered

| Question                                              | Rejected alternative                                                              | Why it was rejected                                                                                            |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Why not hand-written allow lists per agent?           | Separate provider lists, Bash path patterns in hooks                              | Lists drift between the hard boundary and the hook ([0008]).                                                   |
| Why not pin Codex CLI?                                | Pinning Codex in the dev shell, or adding it to required CI                       | Repository tests stay the source of truth, and Codex breakage is reported separately ([0001]).                 |
| Why not vendor Herdr's hooks?                         | Vendored copies (ADR-0002), then a Herdr-specific generator (ADR-0003)            | Vendored files went stale, and each new tool needed its own integration ([0006]).                              |
| Why not replace or deep-merge `settings.json`?        | Immutable symlink, recursive JSON merge, materializing all assets                 | Claude Code also writes the file; owning top-level keys keeps its state without weakening guardrails ([0016]). |
| Why not ship a personal profile?                      | An author-named built-in profile, or a mandatory profile repository               | Personal policy changes would force product releases ([0015]).                                                 |
| Why not put Claude-only rules in the neutral file?    | Provider syntax in `command_permissions.json`, stripping only prefix-shaped rules | The source Claude settings become the authoritative home for Claude-only rules ([0018]).                       |
| Why not infer project-local installs from `--prefix`? | Guessing the runtime scope from the output path                                   | Hooks assumed `$HOME` even when files were written elsewhere; an explicit root is self-contained ([0017]).     |

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
| `provider/spec`                                                                           | Data: provider metadata, asset destinations, and optional `ConfigSync`.                                        |
| `install_plan`                                                                            | Functions: plan publication of rendered staging files and embedded assets.                                     |
| `filesystem`, `install_executor`, `config_sync`                                           | Capabilities: execute file operations and dispatch config synchronization.                                     |
| `nix/providers.json`, `nix/home_manager_module.nix`                                       | Nix provider list generated from the registry, and the Home Manager options, links, and sync built from it.    |
| `generation/hooks`                                                                        | Render hook wiring.                                                                                            |
| `generation/bridges`, `provider/spec`, `install_plan`                                     | Verify bridge asset contracts; declare embedded bridge assets and destinations in specs and plan their writes. |
| `generation/command_permissions`                                                          | Maps neutral rules to Claude, Codex, and Devin permission syntax.                                              |
| `generation/claude_config`, `codex_config`, `devin_config`, `hermes_config`, `dsh_config` | Build and sync provider configs.                                                                               |
| `generation/protection`, `secret_path_policy`                                             | Compute protected paths and secret-file deny rules.                                                            |
| `generation/skills`, `external_hooks`, `hook_bundle`                                      | Render skills and compose external bundles.                                                                    |
| `inventory`                                                                               | Lists the skills and hooks a source provides.                                                                  |
| `runtime_root`                                                                            | Relocates hook commands for project-local installs.                                                            |
| `github_app`                                                                              | Mints GitHub App installation tokens for the agent token option ([0021]).                                      |
| `runtime`                                                                                 | Checks that the manifest's runtime commands are on `PATH`.                                                     |

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
GitHub Actions runs four workflows:

- `ci`: clippy, rustfmt, `cargo test`, and Rust dependency audits, plus calls to the shared
  workflows in `furedea/reusable-workflows` (`nix-lint`, `nix-check`, `gha-lint`,
  `dependency-review`, `claude-review`, `codex-review`), gathered by an `all-green` job.
- `codex_conformance`: generated execpolicy rules against the locked Codex CLI, kept separate from
  `ci` by design ([0001]).
- `release_please`: versioning and changelog.
- `release_artifacts`: cargo-dist release artifacts for a draft release tag, run manually.

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
