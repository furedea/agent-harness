# ADR-0027: Declare Claude and Codex in the provider registry

- Status: Accepted
- Date: 2026-11-09
- Supersedes: ADR-0026

In the context of a provider registry that covered only Devin, Hermes, pi, and dsh while Claude and
Codex kept separate hook, settings, rules, statusline, CLI, inventory, and Home Manager paths, facing
two parallel provider lists that drift, we decided for declaring all six providers in `PROVIDERS`
with the same Data / Functions / Capabilities layering as [ADR-0026](0026_declare_providers_in_a_registry.md),
and against keeping Claude and Codex outside the registry or forcing their settings contracts into
a generic merge algorithm, to get one iteration path for hooks, inventory, protection, installation,
verification, CLI, and Home Manager, accepting some ordering rules and provider-specific render code.

`ProviderSpec` declares what each provider owns: hook, instruction, skill, statusline, and managed
config locations, plus files generated into managed directories. `ConfigSync` gains
`MergeClaudeSettings` and `MergeToml` next to the existing kinds, so the registry declares the sync
kind while `config_sync` and `ProviderRenderer` own the content. Matches on `ProviderId` are exhaustive,
so a new provider fails to compile until it is rendered. `HookProvider` is replaced by `ProviderId`.

`PROVIDERS` lists Codex, then Claude, then the bridged providers, and `ProviderKind::Native` providers
lead installation, synced-directory, and protection lists. This preserves the existing installed
bytes, including the order of `protected_paths.json`, which the complete-source golden checks;
reordering was rejected because that file is a rendered output. Inventory sorts by `ProviderId` so
its output order is independent of the registry order.

`nix/providers.json` now also describes Claude and Codex, and the Home Manager module derives their
options, links, and sync activations from it. `tests/nix_provider_registry.rs` checks the projection
against the Rust registry. Deprecated per-provider commands stay, per
[ADR-0010](0010_use_pre_major_compatibility_versions.md).
