# ADR-0026: Declare providers in a registry

- Status: Accepted
- Date: 2026-11-05

In the context of adding providers across hook generation, installation, inventory, CLI, and
Home Manager, facing parallel provider lists that drift and omit protected paths, we decided
for a declarative `ProviderSpec` registry and Data / Functions / Capabilities layering and
against repeating provider branches in each consumer, to keep provider metadata and managed
paths consistent, accepting a bounded registry and a separately maintained Nix projection.

`src/provider/spec.rs` is Data: `PROVIDERS` declares Devin, Hermes, pi, and dsh, including
hook locations, permission support, embedded asset contents and destinations, managed asset
directories, and optional config sync. Functions render hooks and compute installation operations from those values.
Bridge files are embedded spec assets whose writes are planned by `install_plan`. Capabilities perform filesystem access and config synchronization.
`tests/provider_spec_layering.rs` checks the spec, install planner, hook renderer, and bridge
asset contracts for `fs_ops` and `std::fs` references. This source-text check guards selected modules;
it is not a transitive dependency analysis of all generation code.

Claude and Codex remain outside the registry because their settings, permissions, rules, and
statusline handling have broader specialized contracts. Optional stage 5, moving just their
hooks into the registry, is deliberately deferred rather than forcing those contracts into the
initial spec.

`ConfigSync` is a `ProviderSpec` feature, represented by `config_sync: Option<ConfigSync>`,
rather than another provider-name dispatch table or a callback stored in Data. Its strategies
are `MergeTopLevelJson` (Devin), `MergeYaml` (Hermes), and `RegisterBundle` (dsh); pi has none.
This makes support explicit and keeps execution in `config_sync`, which delegates to the
existing provider implementations. The enum describes the managed operation without pretending
that JSON key replacement, YAML plugin merging, and bundle registration share one merge algorithm.
Adding a new strategy still requires executor code.

The public commands are `generate-hooks --provider <id>` and `sync-config --provider <id>`.
Both accept Claude and Codex through their existing specialized paths as well as registry
providers. `sync-config --provider pi` is rejected because pi has no managed config to sync.
Existing per-provider hook generation and config sync commands remain hidden deprecated aliases,
including `sync-claude-settings`. Keeping them working preserves the pre-major compatibility
boundary established by [ADR-0010](0010_use_pre_major_compatibility_versions.md), which supersedes
[ADR-0009](0009_keep_breaking_releases_pre_major.md); this decision does not remove those commands.

`FileSystem` separates file operations from their real implementation, and `plan_install`
returns `FileOp` values for an executor to apply. It plans publication of an already rendered
staging tree, not generation of the whole installation. Staging generation still writes through
the existing generation path; embedded assets additionally use `plan_assets` and the executor.
This boundary permits fake-filesystem verification of publication without claiming that all I/O
has been removed from generation. Asset destinations live alongside their embedded contents in
the spec so installation and verification do not need separate destination lists.

`nix/providers.json` is the Nix provider list used to generate Home Manager options, links, and
sync activations. Sharing Rust source directly with Nix was rejected in favor of a small JSON
projection checked by `tests/nix_provider_registry.rs` against the Rust registry for provider
identity, metadata, and managed link destinations. Home Manager links explicit managed files
and subdirectories, never entire provider home directories, so user-owned files such as
`~/.hermes/config.yaml` can coexist and be synchronized without link collisions. This accepts
some duplicated declarations, with tests guarding their agreement.

The complete-source golden in `tests/complete_source_golden.rs` preserves installed paths and
contents across the refactor; `tests/provider_cli.rs` checks old/new command equivalence and pi
sync rejection. Future providers still need assets, protocol-specific behavior where necessary,
and the Nix projection rather than a promise that one spec alone implements every integration.

The follow-up removed hand-written provider path lists from layout, protection, and rendering.
Registry-derived paths preserve the existing installation and protection order.
