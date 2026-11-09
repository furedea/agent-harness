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
enum variant, so a new provider fails to compile until every `match` handles it.

- **Hooks**: the source and installed scripts directory, the registration (`HookRegistration`:
  `InSettings(path)` for Claude, `File(path)` for the others), the file external hook bundles write
  to, and the bridge files the hook runtime needs (embedded assets, with an optional directory
  published as a whole).
- **Skills**: the destination and a `SkillsSource` variant.
- **Instructions**: the installed `CLAUDE.md` or `AGENTS.md`.
- **Command rules**: one `CommandRules` enum: `None`, `DevinHookBlock`, `ClaudeRules` (generated
  permission and protected-path files, plus the source files verification requires), or
  `CodexExecpolicy` (one rules file). It replaces a flag and three file lists.
- **Config**: the managed config path and its `ConfigSync` kind, plus the source directories the
  config refers to (Claude's statusline). Claude's `settings.json` and Devin's `config.json` are both
  hook registration and config; the registry states that and publishes the file once.

`ConfigSync` gains `MergeClaudeSettings` and `MergeToml` next to the existing kinds, so the registry
declares the sync kind while `config_sync` and `ProviderRenderer` own the content.
`HookProvider` is replaced by `ProviderId`.

Lists follow registry order, and `ProviderKind` is removed. An earlier design kept Claude and Codex
first to preserve the byte order of `protected_paths.json`; that bought a second classification for
an ordering rule. The one-time cost is that `protected_paths.json`, and the settings and Codex config
lists derived from it, change order. The complete-source golden files are updated, and the change is
checked to be order-only: every file is otherwise identical, and each changed file has the same
lines when sorted. Inventory sorts by `ProviderId`, so its output is independent of registry order.

`nix/providers.json` now also describes Claude and Codex, and the Home Manager module derives their
options, links, and sync activations from it. `tests/nix_provider_registry.rs` checks the projection
against the Rust registry. Deprecated per-provider commands stay, per
[ADR-0010](0010_use_pre_major_compatibility_versions.md).
