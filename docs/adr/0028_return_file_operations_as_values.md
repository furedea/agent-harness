# ADR-0028: Return file operations as values

- Status: Accepted
- Date: 2026-11-12

In the context of three filesystem paths with different guarantees (`filesystem.rs`, `fs_ops.rs`,
and `generation/io.rs`), generators that wrote straight to disk, a renderer that called the real
filesystem, and a layering test that only grepped source text for `fs_ops` and `std::fs`, facing
installed files that were all `0600` and a check that a wrapper such as `io::write_json` bypassed,
we decided for a Functional Core / Imperative Shell split in one crate, enforced by three layers,
and against splitting the crate, to make "computation cannot write" a compile-time fact, accepting
that tests are outside the lint. The [first amendment](#amendment-computation-does-not-read-either)
extends the same split to reads, and the
[second](#amendment-the-boundary-parses-the-source-into-a-typed-model) makes what computation
receives a typed value instead of a bag of files.

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

*The `Snapshot` described here was replaced by a typed `Source` in the
[second amendment](#amendment-the-boundary-parses-the-source-into-a-typed-model).*

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
- **Stages reload.** *(Superseded by the second amendment: the install is planned in one pass.)*
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

## Amendment: the boundary parses the source into a typed model

The first amendment gave computation a `Snapshot`: an in-memory copy of files. Computation still
looked files up by path and parsed raw text where it used it, so a parse error surfaced deep inside
a generator, the request that said what to read could drift from the reads that used it and fail only
at runtime, and the install took a new snapshot per stage because later providers read earlier
providers' output back from disk. The target is a clerk at the window: the boundary reads the
documents and fills out a fixed form, and computation reads only the form.

- **`Source` is the form.** `shell::load_source` reads the source tree, the external hook bundles,
  and the external skills, parses and validates every document once, and returns a `Source`
  (`source_model.rs`, under `forbid`): the manifest, hook config, command permissions, the regex
  policy the hooks are generated from, the secret path policy, the built-in skills with their
  rendering policy, the base Claude settings and Codex config, the source file of each provider
  config that is applied to the installed one, and each bundle's hook files, Codex config, and asset
  list. Every error names the file it came from, and nothing downstream re-validates. Computation
  takes `&Source` and returns `FileOp`s; it has no path-as-lookup and parses no text.
- **Copy-only directories are a location and a file list.** A directory that is only copied (the
  hook scripts, the statusline, a skill's support files, a bundle's scripts) is a `SourceDir`, or a
  path for a skill's support entry, and leaves computation as `FileOp::CopyTree` or `CopyFile`. The
  file list is what protected paths and the bundle conflict check rest on; contents are never loaded.
- **Only the loader knows where things live.** `SnapshotRequest` is gone. The loader is given the
  providers a run renders and the external inputs the command line named (`LoadInputs`), and works
  out the rest from the layout and the registry.
- **Installed configs are loaded the same way.** `shell::load_installed` parses the configs a
  rendering merges into (`InstalledConfigs`); an unparsable one is an error at the boundary. The
  sidecar is not read for the install, which has always merged into the config alone: the sidecar it
  would write sits in the staging directory, which is never published.
- **One pass, in stages.** Install computes the whole plan from `Source` and the installed configs,
  with nothing read between providers (`InstallPlan`, in `install_plan.rs`). Provider-to-provider
  dependencies are values: a provider that copies another's skills renders them again from the same
  `Source` and moves them with `FileOp::relocated`, instead of copying the other's output back from
  disk. The Claude settings are merged into the installed ones while rendering, so the old
  render-then-preserve step is gone. The plan is still applied in stages (synced directories,
  bridge assets, bundle assets, each provider, publication from staging), because a later stage
  writes inside a directory an earlier one replaces and the executor stages every replacement before
  swapping any. A bundle's asset conflicts with a file an earlier stage leaves in the output, which
  the plan works out from the `Source`'s file lists rather than by looking at the staging directory.
- **What stays outside.** No computation module holds a `Snapshot`; the type is deleted. The loader
  and a few shell entry points still read the disk directly, which is their job: `verify_paths`
  (the installed files `verify` checks), `sync` (the rendered and installed configs), and
  `shell::hook_bundle` (what an installer left in its isolated home, parsed into `Staged`).

Behavior that changed with it: a skill must have a `name`, a `description`, and a level-one heading
for any command to load the source, where only `list skills` required the heading before; a
malformed `allowed_commands.json`, provider source config, or bundle hook file is refused by every
command that loads the source, not only by `install`; and a bundle that maps two files to one
installed path is refused when it is loaded. Generated output is unchanged: the golden tests and
`nix/providers.json` are byte-identical.

## Consequences

- Every command that loads the source parses all of it, so a command that needs one document still
    reads the others and fails on any that is invalid. A source is a few dozen small files, and the
    copy-only directories are listed, not read.
- The merge of Claude, Codex, Devin, Hermes, pi, and dsh configs was still per-provider code behind
    `ConfigSync` when this was written; [ADR-0029](0029_declare_config_ownership_and_sync_with_one_engine.md)
    declares ownership in the spec and syncs with one implementation.
- The duplicate provider enums are tracked separately.
