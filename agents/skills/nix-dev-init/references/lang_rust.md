# Phase 2: Rust (cargo)

Prerequisite: repo created via `cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-rust)"`. The template provides `flake.nix`, `.envrc`, `Cargo.toml`, `src/main.rs`, `lefthook.yml`, `.commitlintrc.yml`, CI workflows (lint, format, test, CodeQL), and `.gitignore`. `github/create_repo.sh` also patches `Cargo.toml`'s `name` field and applies GitHub rulesets.

## Steps

1. `direnv allow` — the template already includes `.envrc` (`use flake`).
2. Verify `which cargo` and `which rustc` resolve under `/nix/store/`.
3. `cargo build` — confirms the nix-provided toolchain works end to end.

CI is already scaffolded by the template — skip that offer in the "After Setup" step.

## Why not `rust-overlay` / `fenix` by default

The template uses nixpkgs' `cargo` + `rustc` (stable). This is deliberate:

- Nixpkgs stable is good enough for 95% of projects and avoids an extra flake input.
- `rust-overlay` / `fenix` add a significant first-build cost (the toolchain derivation is large) and lock you to a fresh evaluation every time.
- When a project genuinely needs a specific toolchain channel or nightly feature, add `rust-overlay` as a one-off project exception — not a default.

## LSP / rust-analyzer

Do **not** add `rust-analyzer` to the devShell. The globally-installed `rust-analyzer` (from `~/ghq/github.com/furedea/dotfiles`) discovers the project's toolchain via `rustc --print sysroot`, and because direnv has put the nix-store `rustc` on PATH, the global LSP automatically uses the correct sysroot per project. Adding a per-project `rust-analyzer` bloats the closure for no benefit.

## Common first-run checks

- `cargo --version` and `rustc --version` should resolve under `/nix/store/`.
- `cargo build` should succeed on the starter `main.rs`.
- `cargo clippy` should run without needing extra installs.

## What NOT to do

- Do not run `cargo init` — the template repo already provides `Cargo.toml` and `src/main.rs`. Running `cargo init` overwrites them.
- Do not add `cargo` / `rustc` to `~/ghq/github.com/furedea/dotfiles/nix/home/default.nix`. Per-project pinning is the whole point.
- Do not commit `target/`. It is a machine-specific build cache.
- Do not run `rustup` inside the direnv shell. Nix owns the toolchain; rustup would install a second one into `~/.rustup/` and silently shadow it via `cargo`'s PATH lookup.
