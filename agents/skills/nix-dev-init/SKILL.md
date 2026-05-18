---
name: nix-dev-init
description: >
    Per-project development environment bootstrap workflow for this user: github/create_repo.sh --template → direnv allow → language sync/build inside the direnv-activated shell. ALWAYS load when starting a new project, initializing a repo, scaffolding dev tooling, or writing project-level flake.nix / .envrc / devShell. Also load when the user asks to use or reference template-python, template-typescript, template-rust, template-tex, template-minimal, or configure an existing project from a template. This skill is DISTINCT from `nix-dotfiles` (global dotfiles system config under ghq); load THIS for project-scoped devShells. Without it, Claude or Codex may run language tools on the host shell and leak host toolchain versions into lockfiles. Supported language templates live under furedea/ and are used via `github/create_repo.sh --template`. Also trigger on "/nix-dev-init", "new project", "set up dev environment", or any mention of creating a flake.nix for a project.
---

# Nix Dev Init Workflow

## Scope

Setting up a **per-project** dev environment — not global dotfiles. For home-manager / nix-darwin edits on `~/ghq/github.com/furedea/dotfiles`, use the `nix-dotfiles` skill instead.

The workflow has two phases, and the split is load-bearing:

- **Phase 1 — Nix shell** (this file): `github/create_repo.sh --template` (or manual `flake.nix` → `.envrc`) → `direnv allow`. Language-agnostic.
- **Phase 2 — Language init** (see `references/lang-<name>.md`): verify toolchain, run language-specific sync/build **inside the direnv-activated shell**.

The whole reason Phase 1 runs before Phase 2 is so the language's commands see the nix-provided toolchain on PATH, not the host's. Do not collapse or reorder the phases.

## Mandatory Order (Phase 1)

1. VCS init — pick one:
    - **New repo from template** (preferred for supported languages):
        ```
        cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-<lang>)"
        ```
        `github/create_repo.sh` creates the remote, clones into the ghq root, applies rulesets, and patches config file names. It prints the clone destination; command substitution above moves into that path after it succeeds. The template provides `flake.nix`, `.envrc`, `.gitignore`, `lefthook.yml`, `.commitlintrc.yml`, CI workflows, and language-specific config — **skip steps 2-3**.
    - New repo (public, no template): `gh repo create <name> --public --clone --license MIT` → `cd <name>`
    - New repo (private, no template): `gh repo create <name> --private --clone` → `cd <name>`
    - Clone: `git clone <url>` → `cd <name>`
    - Existing project with git: skip this step
2. `flake.nix` — write the devShell with the language toolchain (skipped for template-repo projects)
3. `.envrc` — single line: `use flake` (skipped for template-repo projects)
4. `direnv allow` — trust the .envrc once, per repo
5. Proceed to Phase 2 via the relevant `references/lang-<name>.md`

### Why this order

- **VCS before flake**: the git repo must exist before `nix flake update` creates `flake.lock`. Use `github/create_repo.sh` for template-backed projects and `gh repo create --clone` for one-off non-template projects instead of `git init` — they set up remote and default branch in one shot. Private repos skip `--license` because an accidental visibility flip with MIT attached grants everyone usage rights.
- **Toolchain before init**: running language init commands (e.g. `pnpm install`) on the host shell picks up the host's toolchain, whose version leaks into lockfiles. On another machine (or in CI) this silently breaks reproducibility.
- **Commit `.envrc` before `direnv allow`**: direnv's trust state is keyed by file hash. Allowing first and editing after immediately invalidates the allow, forcing a re-allow.

## Template Repos

All supported languages have a GitHub template repo under `furedea/`. Use `github/create_repo.sh --template` to create new projects — it clones, applies rulesets, and patches the project name in config files automatically.

| Project type | Template repo | create_repo.sh post-processing | Ship `flake.lock`? |
| --- | --- | --- | --- |
| Python (uv) | `furedea/template-python` | `pyproject.toml` name sub + `ruleset_python` | no |
| TypeScript / Node (pnpm) | `furedea/template-typescript` | `package.json` name sub + `ruleset_typescript` | no |
| Rust | `furedea/template-rust` | `Cargo.toml` name sub + `ruleset_rust` | no |
| TeX / LaTeX | `furedea/template-tex` | `ruleset_tex` only (no name sub) | **yes** (in repo) |
| Fallback (unlisted languages) | `furedea/template-minimal` | base ruleset only | no |

Non-TeX templates intentionally omit `flake.lock`: `nix flake update` runs on first `direnv allow` to resolve a fresh `nixpkgs` commit. The TeX template includes `flake.lock` for reasons explained below.

Do not preemptively generalize to multi-system (`forAllSystems`, `flake-utils`) unless the project actually needs Linux CI. YAGNI.

### Why LSPs are not in the templates

Editor-side tooling (`rust-analyzer`, `pyright`, `typescript-language-server`, …) belongs in the **global** nvim environment, not per-project devShells. When direnv activates the shell it puts the project's `rustc` / `python` / `node` on PATH, and the globally-installed LSP picks those up automatically via `rustc --print sysroot` / `python` discovery. Adding LSPs per project bloats closures for no benefit unless a specific project hits a version mismatch — handle that as a one-off exception, not a default.

### Why the Python template pins uv to the nix interpreter

The `furedea/template-python` flake sets two env vars:

    UV_PYTHON_DOWNLOADS = "never";
    UV_PYTHON_PREFERENCE = "only-system";

These force uv to use the `python314` that nix puts on PATH instead of silently downloading a `python-build-standalone` binary from GitHub into `~/.local/share/uv/python/`. Nix stays the single source of truth for the interpreter; uv is reduced to package resolution, lockfile, and venv management. If nix's Python is too old for `requires-python` in `pyproject.toml`, uv fails loudly — that is the correct failure mode (better than a silent fallback that leaks a non-nix interpreter into the project).

### TeX: `flake.lock` is checked in on purpose

TeX Live output is sensitive to package versions — a tlpdb update can silently change typeset output or break `chktex` / `tex-fmt` — so TeX projects pin to an exact `nixpkgs` commit rather than a branch ref. The `furedea/template-tex` repo tracks `nixpkgs-unstable` (not `nixpkgs-25.11-darwin` like the others) because TeX Live updates land on unstable first; the lock is what makes "unstable + reproducible" coherent.

The canonical `flake.lock` lives in `~/dev/tex/shigyo/`. If you ever run `nix flake update` in `shigyo`, also update `furedea/template-tex`'s `flake.lock` so freshly-initialized TeX projects stay aligned.

## .envrc

All template repos include `.envrc` with a single line: `use flake`. For non-template projects (fallback path), create `.envrc` manually with the same content.

The devShell in `flake.nix` is the single source of truth for PATH and env. Adding `dotenv`, `PATH_add`, or inline exports to `.envrc` fragments that truth — a week later you will not remember whether a var came from flake or envrc, and reproducing the env elsewhere means diffing two files.

If the project genuinely needs secrets, put them in a separate `.env` (ignored) and add a single `dotenv .env` line. Keep the shell definition in flake regardless.

## Ignore Rules

All template repos include `.gitignore` with direnv cache and nix build outputs:

    .direnv/
    result
    result-*

plus language-specific entries. For non-template projects (fallback path), add these lines manually.

## Phase 2: Language Init

After `direnv allow`, hand off to the language-specific reference. Each ref covers verification, sync/build, and language-specific anti-patterns.

| Project type | Reference | Downstream skill |
| --- | --- | --- |
| Python (uv) | `references/lang_python.md` | `python-style` |
| TypeScript / Node (pnpm) | `references/lang_typescript.md` | — |
| Rust | `references/lang_rust.md` | — |
| TeX / LaTeX | `references/lang_tex.md` | — |
| Unlisted languages | `references/lang_fallback.md` | — |

Read only the ref that matches the project's primary language — the files are intentionally standalone so Phase 2 loads one language's context, not all of them.

## After Setup

- CI is already scaffolded by the template — skip the `github-ci-init` offer for template-repo projects
- Development follows TSDD, detailed in the tsdd skill
- Language conventions are in the corresponding \*-style skill

## Anti-Patterns

- Using `git init` instead of `github/create_repo.sh` or `gh repo create --clone` for new projects → remote URL hand-typing, branch name mismatch (`master` vs `main`), missing license/gitignore.
- Running `uv init` / `pnpm install` / `cargo build` on the host shell before `direnv allow` → host toolchain leaks into the project.
- Adding project-only tooling to `~/ghq/github.com/furedea/dotfiles/nix/home/default.nix` → bloats the global user env; keep project tooling in the project's own flake.
- Running `darwin-rebuild switch` after editing a project's `flake.nix` → unnecessary. `darwin-rebuild` only reads the dotfiles flake + the nix-darwin modules.
- Editing files under `.direnv/` by hand → it is a cache; change `flake.nix` instead and let direnv rebuild it on next `cd`.
- Manually scaffolding files that the template repo already provides (e.g. running `pnpm init` when `template-typescript` already has `package.json`).

## Verification

After `direnv allow`, `cd` into the repo should print:

    direnv: loading ~/project/.envrc
    direnv: using flake
    direnv: export ~PATH ...

If nothing happens:

1. `direnv status` — is direnv blocked or not hooked into the shell?
2. Did `direnv allow` succeed? (it hashes the current `.envrc`)
3. Does the flake evaluate? `nix develop --command env | head`
