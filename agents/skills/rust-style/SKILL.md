---
name: rust-style
description: >
    Rust coding conventions: Cargo commands, package layout, tests, rustfmt/clippy, anyhow/thiserror error boundaries, newtypes, enums, ownership, Path/PathBuf filesystem rules, serde config handling, naming, comments, and docs. Load whenever writing, reviewing, refactoring, linting, testing, or discussing Rust code or Rust project style inside an already-bootstrapped project. Do NOT use for initial Nix, direnv, or Cargo project bootstrap; use nix-dev-init first.
---

# Rust Coding Style Guidelines

## Scope

This skill governs code written inside an already-bootstrapped Rust project: package commands, module layout, test authoring, refactoring, code review, naming, ownership, errors, filesystem operations, comments, and docs.

Project bootstrap (flake.nix, direnv, initial `Cargo.toml`, initial `rust-toolchain.toml`) belongs to the `nix-dev-init` skill. If the project is not yet bootstrapped, defer to `nix-dev-init` first and return here once `direnv allow` succeeds and Cargo is available.

### Why the split

Bootstrap is an environment concern. This skill is for day-to-day Rust implementation inside an existing project. Do not duplicate Nix, direnv, or initial Cargo setup rules here.

## Package Management

- Use Cargo for Rust package operations.
- Add dependencies with `cargo add <crate>` when `cargo add` is available.
- Add development dependencies with `cargo add --dev <crate>`.
- Use `cargo check` for a fast compile/type-check pass when tests are not needed yet.
- Use `cargo test` for the default verification pass.
- Use `cargo clippy --all-targets -- -D warnings` for linting.
- Do not edit `Cargo.lock` manually.
- Commit `Cargo.lock` for applications, CLI tools, and internal tools.
- For library crates, follow the repository's existing `Cargo.lock` policy.
- Do not use `@latest`-style version shortcuts in documentation or committed commands.
- Do not add a dependency only because it is common. Add it when it removes real boundary complexity or prevents error-prone code.

## Directory Structure

- Follow Cargo's standard layout.
- Store production code in `src/`.
- Keep `src/main.rs` thin for binary crates. It should parse CLI arguments and call library code.
- Put reusable execution logic in `src/lib.rs` and focused modules under `src/`.
- Store integration tests in `tests/`.
- Keep unit tests near the module when they need private access.
- Use `examples/` only for runnable examples that should compile.
- Avoid `utils.rs` and `helpers.rs`; name modules by domain or action.

Typical binary crate shape:

```text
src/
├── main.rs
├── lib.rs
├── cli.rs
├── config.rs
├── fs_ops.rs
└── render.rs
tests/
└── cli.rs
```

## File Standards

- Let `rustfmt` define formatting. Do not hand-format around rustfmt.
- Keep files focused and cohesive.
- Prefer modules of roughly 200-500 lines. Split when multiple responsibilities appear.
- Keep public items before private helpers when it improves scanning.
- Use `mod.rs` only when the existing project already uses that style; otherwise prefer `module_name.rs` plus `module_name/child.rs`.
- Put one top-level concept per file when the concept has real behavior.
- Keep generated code out of hand-written modules unless the project has a clear generated-code convention.

## Testing

- Use `cargo test` as the default test command.
- Write unit tests in the target source file or module with `#[cfg(test)] mod tests`.
- Use `tests/*.rs` for integration tests that exercise public APIs or CLI behavior.
- Integration tests compile as a separate crate and should use only public APIs.
- Use doc tests for public examples that should stay compilable.
- Test edge cases and error paths for new behavior.
- Add regression tests for bug fixes.
- Use temporary directories for filesystem tests. Do not touch the real home directory.
- Snapshot generated artifacts only when exact output is part of the behavior.
- Keep snapshots small and focused.
- Do not test behavior that the type system already guarantees.
- Prefer fake implementations behind traits over broad mocking libraries.
- Use table-driven tests for the same behavior across multiple inputs.
- Use async tests only when the project already has an async runtime requirement.

### Test Structure

- **Unit test**: place next to the implementation in the same file or module.
- **Integration test**: place under `tests/` when testing public API, CLI behavior, or crate-level wiring.
- **Helper function**: prefer local helper functions when setup is lightweight or argument-driven.
- **Shared fixture**: keep it in the test module first; move to `tests/common/` only when multiple integration test files need it.
- **Fake implementation**: prefer explicit fake types over global mocks.
- **Parameterized cases**: use a table of cases and loop over it when one behavior has many inputs.

Example unit test layout:

```rust
pub fn normalize_name(input: &str) -> String {
    input.trim().replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_name_trims_whitespace_and_replaces_underscores() {
        assert_eq!(normalize_name(" rust_style "), "rust-style");
    }
}
```

## Syntax Rules

### Module Item Order

- Put module-level constants before types when they configure the following code.
- Prefer `type` aliases and enums before structs that use them.
- Put public structs, enums, and traits before their implementations.
- Put public functions before private helper functions.
- Keep `#[cfg(test)] mod tests` at the bottom of the file.
- Keep related types and their `impl` blocks close together.

### Structs

- Derive `Debug` for most domain structs.
- Derive `Clone`, `PartialEq`, `Eq`, `Ord`, or `Hash` only when the type actually needs that capability.
- Keep fields private when the type has invariants.
- Use constructors for types with validation or normalization.
- Use tuple structs for narrow newtypes and named-field structs when field names improve clarity.
- Avoid boolean constructor arguments; prefer an enum or options struct.

Value Object pattern:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillName(String);

impl SkillName {
    pub fn parse(value: impl Into<String>) -> anyhow::Result<Self> {
        let value = value.into();
        if value.is_empty() {
            anyhow::bail!("skill name must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

### Enums

- Use enums for fixed states, providers, modes, and policy decisions.
- Convert user input strings into enums at the boundary.
- Avoid passing raw string modes through the codebase.
- Prefer exhaustive `match` statements for domain control flow.
- Implement `Display` when the enum has a stable user-facing representation.
- Implement `FromStr` or `TryFrom<&str>` when parsing user input.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Codex,
}
```

### Functions and Methods

- Keep functions focused and small.
- Prefer borrowing in parameters: `&str`, `&Path`, `&[T]`.
- Return owned values when the function creates or transforms ownership.
- Use associated functions for pure construction.
- Keep I/O in module-level functions or dedicated service structs, not inside Value Object constructors.
- Prefer `Result<T, E>` for expected failures.
- Prefer early returns for guard clauses.
- Avoid `async` unless the project has real concurrency or nonblocking I/O requirements.

### Getter and Conversion Naming

- Borrowed views use `as_*`: `as_str`, `as_path`, `as_slice`.
- Consuming conversions use `into_*`: `into_inner`, `into_path_buf`.
- Cheap copies can use the value name directly: `len`, `status`, `provider`.
- Avoid Java-style `get_*` unless following an existing local convention.
- Boolean queries use `is_*`, `has_*`, `can_*`, or `should_*`.
- Fallible parsing uses `parse`, `try_from`, or `from_str`.

### Ownership and Borrowing

- Accept `impl AsRef<Path>` only at outer convenience boundaries. Inside the codebase, pass `&Path`.
- Accept `impl Into<String>` for constructors that store owned strings.
- Do not clone to satisfy the borrow checker until the ownership model is understood.
- Use `Cow` only when profiling or API shape shows it is worthwhile.
- Prefer slices over `&Vec<T>` in function parameters.

### Collections

- Use `Vec<T>` for ordered sequences.
- Use `BTreeMap` or sorted vectors when deterministic output order matters.
- Use `HashMap` when order is irrelevant and lookup dominates.
- Do not expose mutable collections directly from domain types.

### Strings and Formatting

- Use `&str` for borrowed string input and `String` for owned string storage.
- Use `format!` when constructing a new owned string from values.
- Use captured identifiers in formatting when it improves readability: `format!("{name}")`.
- Prefer `to_owned()` or `String::from` when converting a string literal to `String`.
- Avoid unnecessary `to_string()` in hot or repeated code.
- Keep user-facing text separate from machine-readable output.

### Numeric Conversions

- Use `usize` for indexing and collection lengths.
- Do not use `as` for narrowing integer conversions.
- Use `TryFrom` or `try_into` when a conversion can fail.
- Use newtypes when a number has a domain unit or invariant.
- Make lossy conversions explicit in the function or variable name.

## Error Handling

- Use `anyhow::Result` at application and CLI orchestration boundaries where errors are reported to humans and callers do not branch on error categories.
- Use concrete error types when callers or tests need to distinguish failure categories.
- `thiserror` is appropriate for deriving concrete error types. Do not introduce it just to wrap every possible failure.
- Add context at I/O, external command, parse, and config boundaries.
- A low-level error such as "No such file or directory" must include the operation and path.
- Use `?` for propagation.
- Do not use `unwrap` or `expect` in production code except for impossible states justified by a nearby invariant.

### Panic Policy

- Use `Result` for expected failures.
- Reserve `panic!` for logic bugs and impossible states.
- Avoid direct indexing when missing data is a normal possibility; use `get()` or explicit validation.
- Public APIs that can panic must document `# Panics`.
- Do not leave `todo!`, `unimplemented!`, or debugging `panic!` calls in production code.
- Use `unreachable!` only when the type system or previous validation makes the branch impossible.

Application-boundary error:

```rust
use anyhow::{Context, Result};

pub fn read_config(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))
}
```

Distinguishable domain error:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
}
```

## CLI Code

- Use typed structs and enums for CLI input.
- Keep argument parsing separate from execution.
- Do not perform filesystem or network work while parsing CLI arguments.
- Human-readable command results go to stdout.
- Progress, warnings, and diagnostics go to stderr.
- Machine-readable output must not be mixed with human logs.
- Keep exit codes coarse but meaningful enough for CI.

```rust
#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    Render(RenderArgs),
    Verify(VerifyArgs),
}
```

## Logging and Diagnostics

- Short-lived CLI tools may use `eprintln!` for warnings and progress.
- Libraries must not initialize global logging.
- Use `tracing` only when the project needs structured logs, spans, or long-running diagnostics.
- Keep human diagnostics on stderr and machine-readable command output on stdout.
- Include paths, command names, and config keys in diagnostics when they explain the failure.

## Filesystem Operations

- Use `Path` and `PathBuf` for paths. Do not build paths with string concatenation.
- Treat source tree traversal and symlink creation as separate behaviors.
- Do not follow symlinks while traversing source trees unless the behavior is explicitly required and tested.
- Use `DirEntry::file_type()` instead of `Path::is_dir()` when symlink behavior matters.
- Keep file-writing operations behind small functions that are easy to test with temporary directories.
- Distinguish files fully owned by the project from files also modified by users or external tools.
- Prefer writing to a temporary file and replacing the target when partial writes would be harmful.

Symlink-aware traversal without following symlinks:

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn collect_regular_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_regular_files_into(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_regular_files_into(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_regular_files_into(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
```

## Config and Serialization

- Use typed structs for project-owned config.
- Use `serde(deny_unknown_fields)` where unknown input keys should be rejected.
- Do not reject unknown keys in external-tool-owned config files.
- Use `#[serde(rename_all = "...")]` to keep serialized field naming consistent.
- Use `#[serde(default)]` for optional input fields that have stable defaults.
- Use `#[serde(skip_serializing_if = "Option::is_none")]` when absent values should not appear in output.
- Use `#[serde(flatten)]` sparingly because it makes schemas less explicit.
- Separate fully generated files from user-owned files that receive managed updates.
- Do not overwrite user-owned or external-tool-owned config files wholesale.
- For managed config sync, update only the keys owned by the project and preserve unknown keys.
- Use stable ordering for generated output when review diffs or tests depend on order.
- Use a format-preserving edit strategy when comments, ordering, and unknown future keys must be preserved.

## Cargo Features

- Keep feature flags additive. Enabling a feature should not remove behavior.
- Avoid growing default features unless the behavior is truly the default user expectation.
- Keep optional dependency features named clearly after the capability they enable.
- Test important feature combinations explicitly.
- Consider `cargo test --all-features` when features affect compiled code paths.
- Do not use features as a runtime configuration substitute.

## Public API Design

- Keep public APIs smaller than internal APIs.
- Avoid exposing third-party types in public APIs unless that dependency is part of the intended API.
- Use `#[non_exhaustive]` for public enums or structs that may need new variants or fields.
- Seal traits that downstream crates should not implement.
- Treat new public trait implementations as semver-relevant changes.
- Do not expose mutable internal collections directly.

## Macro and Build Script Policy

- Prefer functions, traits, and generics before macros.
- Add a macro only when it removes repetition that ordinary Rust abstractions cannot express clearly.
- Avoid procedural macros unless the project has a strong reason for compile-time code generation.
- Keep test helper macros small and obvious.
- Do not add `build.rs` until native linking, environment probing, or code generation is actually required.
- Generated code should have tests or snapshots that make the generated output reviewable.

## Naming Conventions

- Modules and files: `snake_case`.
- Functions, methods, and variables: `snake_case`.
- Types, traits, and enum variants: `UpperCamelCase`.
- Constants and statics: `SCREAMING_SNAKE_CASE`.
- Generic type parameters: short `UpperCamelCase` names such as `T`, `E`, `P`.
- Lifetimes: short lowercase names such as `'a`; use descriptive names only when they clarify an unusual relationship.
- Cargo feature names: `kebab-case`.
- Avoid `manager`, `helper`, `util`, and `common` unless the name is already established locally.

### Module Naming

- Name modules after the domain or action they contain.
- Prefer `render.rs` over `renderer.rs` for module names.
- Prefer `install.rs` over `installer.rs` for module names.
- Prefer `config.rs` over `config_manager.rs` for module names.
- Use `Renderer` or `Installer` as type names only when the type owns behavior.

## Imports

- Group imports as standard library, third-party crates, then local crate modules.
- Let rustfmt sort and format imports when the project enables that behavior.
- Prefer explicit imports over glob imports.
- Use glob imports only in tests or prelude-style modules where the scope is obvious.
- Avoid `as` aliases unless they remove ambiguity or follow a local convention.
- Prefer `crate::` for crate-local imports and `super::` for parent-module test imports.
- Keep trait imports close to the code that needs method resolution.
- Do not use glob imports in production modules except for explicit prelude modules.

## Whitespace and Line Breaks

- Let rustfmt make final whitespace and wrapping decisions.
- Use trailing commas in multi-line structs, enums, arrays, match arms, and function calls.
- Break long method chains at semantic boundaries.
- Do not fight rustfmt with manual alignment.
- Keep one blank line between groups of related items when it improves scanning.

## Comments and Documentation

- Comments explain why, not what.
- Prefer clear types, function names, and module boundaries over explanatory comments.
- Public APIs should have doc comments when they are consumed outside the crate.
- Include examples in doc comments only when they should compile as doc tests.
- Use `//!` for module-level documentation.
- Use `///` for item documentation.
- Public fallible functions should document `# Errors` when the error cases are not obvious.
- Public functions that can panic should document `# Panics`.
- Architectural choices, dependency decisions, and external-tool boundary rules belong in ADRs.
- Do not write prose specifications that duplicate tests.

## Quality Gates

Run these before finishing Rust changes unless the project defines a stricter gate:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- Forbid `unsafe` by default.
- Do not enable broad lint sets such as `clippy::pedantic` at project start.
- Add targeted lint configuration only when it catches mistakes the project actually cares about.

Recommended package-level guard:

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```
