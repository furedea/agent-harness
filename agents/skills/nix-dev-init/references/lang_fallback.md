# Phase 2: Fallback (language without a dedicated template)

Use this path when the project's primary language is not Python, TypeScript/Node, Rust, or TeX. Prerequisite: repo created via `cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-minimal)"` (or `gh repo create` without a template). `template-minimal` provides `flake.nix` (empty `packages` list), `.envrc`, `.commitlintrc.yml`, `.gitignore`, and base CI workflows (gha_lint, dependency_review).

The goal of this ref is to keep the fallback path **principled**, not to enumerate every language. Do not preemptively create a new template repo just because a project happened to use that language — wait until the language appears multiple times and a pattern is clear. YAGNI.

## Steps

1. `direnv allow` — the template already includes `.envrc` (`use flake`).
2. Identify the language's canonical nix package names for: compiler/runtime, package manager (if separate), formatter, linter. Use `nix search nixpkgs <name>` or check `search.nixos.org`.
3. Edit `flake.nix` and fill in `devShells.${system}.default.packages` with those derivations. Prefer names that already exist on stable nixpkgs over overlays.
4. `direnv reload` (or just `cd .` after editing the flake — direnv auto-rebuilds on `flake.nix` mtime change).
5. Verify the toolchain is on PATH under `/nix/store/` with `which <tool>`.
6. Run the language's standard init command inside the direnv-activated shell — never on the host shell. Examples: `go mod init`, `mix new`, `dotnet new console`, `zig init-exe`, `elm init`, `deno init`.
7. Add language-specific ignore entries to `.gitignore` on top of the entries from the template.

## Principles that still apply

Everything in Phase 1 still holds regardless of language:

- **Toolchain before init**: run the init command _after_ `direnv allow`, never before. The whole reason Phase 1 exists is to prevent the host toolchain from leaking into project lockfiles / manifests.
- **One source of truth**: the devShell in `flake.nix` is the only place the toolchain version should live. Do not duplicate it in `.envrc` via `PATH_add` or inline exports.
- **No LSPs in the devShell**: globally-installed LSPs (in `~/ghq/github.com/furedea/dotfiles`) discover the project toolchain via PATH once direnv activates. Per-project LSPs bloat the closure for no benefit.
- **No host-shell escape hatches**: if "just brew install it" is tempting, stop — that reintroduces the exact host-leak problem this skill exists to prevent.

## When to promote a fallback into a template repo

If the same fallback recipe is repeated across ≥3 projects with only trivial variation, it is time to:

1. Create `furedea/template-<lang>` from `furedea/template-minimal` via `github/create_repo.sh`.
2. Add language-specific `flake.nix`, config files, CI workflows, and `.gitignore` entries.
3. Create `~/ghq/github.com/furedea/dotfiles/github/ruleset_<lang>.json` with the required CI status checks.
4. Add the post-processing case to `github/create_repo.sh` for name substitution and ruleset application.
5. Create `references/lang-<lang>.md` following the structure of the existing language refs.
6. Update the template table in `SKILL.md` and this skill's description.

Do not do this for a single project. The overhead of maintaining a template repo that only has one caller is worse than the fallback path.

## What NOT to do

- Do not add the language's toolchain to `~/ghq/github.com/furedea/dotfiles/nix/home/default.nix` "temporarily". Temporary global installs have a way of becoming permanent.
- Do not create a new template repo on the first use of a language. Wait for the pattern.
- Do not skip Phase 1 and go straight to `go mod init` / `mix new` on the host shell "because it's a quick experiment". Experiments that aren't worth a flake aren't worth committing either — use `nix shell nixpkgs#go` as a throwaway instead.
