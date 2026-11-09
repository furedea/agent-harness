# ADR-0027: Declare Claude and Codex in the provider registry

- Status: Accepted
- Date: 2026-11-09
- Supersedes: ADR-0026

In the context of a provider registry that covered only Devin, Hermes, pi, and dsh while Claude and
Codex kept separate hook, settings, rules, statusline, CLI, inventory, and Home Manager paths, facing
two parallel provider lists that drift, we decided for declaring all six providers in `PROVIDERS`
with the same Data / Functions / Capabilities layering as [ADR-0026](0026_declare_providers_in_a_registry.md),
and against keeping Claude and Codex outside the registry, forcing their settings contracts into
a generic merge algorithm, or giving a provider fields only it has, to get one iteration path for
hooks, inventory, protection, installation, verification, CLI, and Home Manager, accepting
provider-specific render code.

`ProviderSpec` has five concepts, and every field belongs to one of them. A provider difference is an
enum variant, so a new provider fails to compile until every `match` handles it. Every provider has
hooks, so `hook` is required; the other concepts are an `Option` that is `None` when a provider does
not have them, instead of a `None` variant inside each type.

- **Hook** (`HookSpec`): the source and installed scripts directory, the registration
  (`HookRegistration`: `InSettings(path)` for Claude, `File(path)` for the others), the file
  external hook bundles write to, and the bridge files the hook runtime needs (embedded assets, with
  an optional directory published as a whole).
- **Skill** (`SkillSpec`): the destination and a `SkillSource` variant.
- **Instructions**: the installed `CLAUDE.md` or `AGENTS.md`.
- **Command rule** (`CommandRuleSpec`): `DevinHookBlock`, `ClaudeRules` (generated permission and
  protected-path files, plus the source files verification requires), or `CodexExecpolicy` (one
  rules file). It replaces a flag and three file lists.
- **Config** (`ConfigSpec`): the managed config path and its `ConfigSync` kind, plus the source
  directories the config refers to (Claude's statusline). Claude's `settings.json` and Devin's
  `config.json` are both hook registration and config; the registry states that and publishes the
  file once.

`ConfigSync` gains `MergeClaudeSettings` and `MergeToml` next to the existing kinds, so the registry
declares the sync kind while `config_sync` and `ProviderRenderer` own the content.
`HookProvider` is replaced by `ProviderId`.

Lists follow registry order, and `ProviderKind` is removed. An earlier design kept Claude and Codex
first to preserve the byte order of `protected_paths.json`; that bought a second classification for
an ordering rule. The one-time cost is that `protected_paths.json`, and the settings and Codex config
lists derived from it, change order. The complete-source golden files are updated, and the change is
checked to be order-only: every file is otherwise identical, and each changed file has the same
lines when sorted. Inventory sorts by `ProviderId`, so its output is independent of registry order.

The Rust registry is the single source, and `nix/providers.json` is generated from it. The Home
Manager module reads the JSON to derive each provider's options, links, and sync activations, so Nix
cannot declare a provider the registry lacks. `tests/nix_provider_registry.rs` is a sourcegen-style
test, as in rust-analyzer: it renders the JSON from `PROVIDERS` and fails when the committed file
differs, and `UPDATE_PROVIDERS_JSON=1` rewrites it (see `CONTRIBUTING.md`). A hand-written JSON
checked by an equality test was rejected because every provider change meant editing two files that a
test then had to reconcile. The projection derives activation names, commands, and sync targets from
the `ConfigSync` kind, and the option text from `harness_name`. Deprecated per-provider commands
stay, per [ADR-0010](0010_use_pre_major_compatibility_versions.md).
