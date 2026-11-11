# ADR-0028: Return file operations as values

- Status: Accepted
- Date: 2026-11-12

In the context of three filesystem paths with different guarantees (`filesystem.rs`, `fs_ops.rs`,
and `generation/io.rs`), generators that wrote straight to disk, a renderer that called the real
filesystem, and a layering test that only grepped source text for `fs_ops` and `std::fs`, facing
installed files that were all `0600` and a check that a wrapper such as `io::write_json` bypassed,
we decided for a Functional Core / Imperative Shell split in one crate, enforced by three layers,
and against splitting the crate, to make "computation cannot write" a compile-time fact, accepting
that tests are outside the lint and that reads are not yet banned.

## Pattern

- **Computation returns values.** `generation`, `provider::spec`, `provider::renderer`, and
    `install_plan` return `FileOp` values that say which file gets what content: `WriteFile`,
    `CopyFile`, `CopyTree` (replace a directory with a copy of another), and `ReplaceTree` (replace a
    directory with exactly the listed contents). Sync functions return the operations that update the
    user-owned config instead of rewriting it.
- **The executor is the only writer.** `shell::execute` applies a list all-or-nothing, as
    [ADR-0016](0016_materialize_mutable_claude_settings.md)'s install already did: every replacement
    is staged beside its target, then swapped in by rename, and a failure restores what was swapped
    and removes the directories it created. The staging directory and `config sync` use the same path,
    so atomicity no longer differs by caller.
- **One port.** `shell::io` holds the filesystem trait and its real implementation. Its contract:
    nothing is written in place; `FileMode::Standard` files get the process's default mode,
    `FileMode::Private` files are `0600` in `0700` directories (GitHub App token files), and
    `FileMode::Bits` reproduces a packaged file exactly. Copies keep their source's permission bits.
    Before this change every installed file was `0600`, so generated config now follows the umask.
- **Providers render in order.** Each provider's operations are applied before the next provider
    renders, because a provider may read what an earlier one wrote (copied skills, the merged Codex
    config).

## Enforcement

Each layer closes a hole the previous one leaves open.

1. **`clippy.toml` `disallowed-methods` / `disallowed-types`** bans std's write APIs
    (`std::fs::write`, `copy`, `create_dir*`, `remove_*`, `rename`, `hard_link`, `set_permissions`,
    `File::create`, `OpenOptions::open`, `DirBuilder::create`) and the `File`, `OpenOptions`, and
    `DirBuilder` types. Only `shell::io` lifts them, with `#![expect(...)]` and a reason, so the
    exemption fails to compile if it stops being needed. `build.rs` runs before the crate exists and
    carries its own `#[allow]`.
2. **Module visibility.** The port is `mod io` inside `shell`, and its items are
    `pub(in crate::shell)`. A computation module cannot name it, so it cannot write through a
    wrapper of its own. Only `execute`, `TemporaryDirectory`, and the sync and hook-bundle
    entry points leave `shell`.
3. **`#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]`** at the top of
    `generation`, `provider::spec`, `provider::renderer`, `install_plan`, and `file_op`. A computation
    module cannot `#[allow]` the ban again. `forbid` also rejects macro-generated `allow(clippy::style)`,
    so `ProviderId` implements clap's `ValueEnum` by hand in `provider/command_line.rs` instead of
    deriving it.

`tests/provider_spec_layering.rs` is deleted: it was a substring check on four files, and it could
not see a wrapper. CI runs `cargo clippy -- -D warnings`, which skips `#[cfg(test)]` code, so test
fixtures may write freely; tests inside computation modules use `shell::fixtures` so the code still
compiles under `forbid` if someone lints test targets.

## Why not split the crate

A `core` crate that cannot depend on I/O would make the boundary structural, but:

- `std::fs` is callable from every crate, so the clippy ban is needed in the core crate anyway. The
    split would replace only layer 2, which module visibility already provides.
- It costs a workspace migration, a redesigned public API between the crates, and changes to
    cargo-dist, release-please, and the Nix package.

Revisit it if the core must be reused by another program.

## Consequences

- Reads are not banned. Computation still reads the source tree and installed configs, so its result
    can depend on the disk at the moment it runs. Loading a validated snapshot at the boundary, and
    banning `std::fs` reads, `Path::exists`, `std::env::var`, and `std::process::Command` in
    computation, is the next step.
- The merge of Claude, Codex, Devin, Hermes, pi, and dsh configs is still per-provider code behind
    `ConfigSync`. Declaring ownership in the spec and syncing with one implementation is a later step.
- `ProviderId` has a hand-written `ValueEnum`; the duplicate provider enums are tracked separately.
