# ADR-0028: Return file operations as values

- Status: Accepted
- Date: 2026-11-12

In the context of three filesystem paths with different guarantees (`filesystem.rs`, `fs_ops.rs`,
and `generation/io.rs`), generators that wrote straight to disk, a renderer that called the real
filesystem, and a layering test that only grepped source text for `fs_ops` and `std::fs`, facing
installed files that were all `0600` and a check that a wrapper such as `io::write_json` bypassed,
we decided for a Functional Core / Imperative Shell split in one crate, enforced by three layers,
and against splitting the crate, to make "computation cannot write" a compile-time fact, accepting
that tests are outside the lint. The [amendment](#amendment-computation-does-not-read-either)
extends the same split to reads.

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
    `FileMode::Bits` reproduces a packaged file exactly. Copies keep their source's permission bits,
    and a regular file that already exists keeps its own bits when replaced, so a sync never widens
    a config the user restricted (one holding API keys, say). Before this change every installed
    file was `0600`; now only new generated files follow the umask.
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
    so `ProviderId` lives in `provider/id.rs`, which keeps clap's `ValueEnum` derive under
    `#![deny(...)]` instead; the registry test still enumerates every variant through the derive.

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

## Amendment: computation does not read either

A computation that reads mid-way depends on the disk at that instant, so one input could yield two
outputs. The same split now covers reads.

- **Read once, at the boundary.** `shell::load` takes a `SnapshotRequest` (directories to walk,
  directories to list, files to read as text, paths to probe) and returns a `Snapshot`: every path
  with its kind (symlinks resolved as `Path::is_file` would) and the UTF-8 text of the documents
  computation parses. A tree records text only for `json`, `toml`, `yaml`, `yml`, and `md`; a named
  document is read whatever its extension. Whole directories that are merely copied need no contents:
  computation returns `FileOp::CopyTree` with the source path, and only the listing it decides on
  is in the snapshot.
- **Computation queries a value.** `Snapshot` and `Tree` (a root plus its snapshot) live in
  `snapshot.rs`, which is itself under `forbid`. `generation`, the renderer, and the sync functions
  receive a `Tree` or a `ConfigFiles` (rendered config, user config, snapshot) instead of a path.
  The loader walks a symlinked directory one hop and never a symlink met inside it, and
  `regular_files` skips what lies behind a symlink, as the directory walk it replaced did, so a
  profile may share a `hooks` directory without its protected paths vanishing.
- **Stages reload.** The install still renders in stages whose later stages need what earlier ones
  wrote (the carried-over configs, the files a bundle's assets must not collide with). Each stage
  takes its own snapshot after the previous one is applied; computation stays a function of one
  snapshot.
- **The ban.** `clippy.toml` also bans `std::fs::{read, read_to_string, read_dir, metadata,
  symlink_metadata, canonicalize, read_link}`, `Path::{exists, try_exists, is_file, is_dir,
  is_symlink, metadata, symlink_metadata, read_dir, read_link, canonicalize}`,
  `std::env::{var, var_os, vars, vars_os, current_dir, current_exe, home_dir}`, and
  `std::process::Command::new`. `forbid` now also covers `snapshot`, `layout`, `manifest`,
  `inventory`, and `render`.
- **Who may read.** `shell::load` and `shell::io` are the port. The remaining imperative modules
  lift the ban for themselves with `#![expect(clippy::disallowed_methods, reason = "...")]`: `cli`
  (`HOME`), `source` (environment, working directory, executable), `runtime` (`PATH` and file
  modes), `github_app` (the key and the `gh` config it renews), `shell::hook_bundle` (it runs the
  installers), and `build.rs`. This is smaller than routing each through the port: their reads are
  the point of the module, not an input to a computation, and `expect` fails to compile when one
  stops reading.

## Consequences

- A snapshot holds raw text, and computation still parses a document where it uses it. Parsing is
    deterministic, so it cannot make a result depend on the disk; parsing once into constrained types
    at the boundary is left for the sync step.
- A snapshot walks the whole source tree, so a very large source directory costs a read of its
    documents even when a command needs only one.
- The merge of Claude, Codex, Devin, Hermes, pi, and dsh configs is still per-provider code behind
    `ConfigSync`. Declaring ownership in the spec and syncing with one implementation is a later step.
- The duplicate provider enums are tracked separately.
