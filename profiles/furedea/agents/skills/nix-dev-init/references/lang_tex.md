# Phase 2: TeX / LaTeX

Prerequisite: repo created via `cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-tex)"`. The template provides `flake.nix`, `flake.lock`, `.envrc`, `lefthook.yml`, `.commitlintrc.yml`, CI workflows (tex-fmt lint + chktex), and `.gitignore`. The TeX template is the only one that ships `flake.lock` — see this skill's main workflow for why.

TeX does not have an `init` command in the usual sense; Phase 2 here is about project scaffolding and verifying the typesetting chain, not a package manager bootstrap.

## Steps

1. `direnv allow` — the template already includes `.envrc` (`use flake`).
2. Verify `which xelatex` resolves under `/nix/store/`.
3. Create the source entry point — typically `main.tex` — or `cp` an existing project skeleton if one is available.
4. Decide on a build driver:
    - `latexmk` (recommended, handles bib / rerun automatically)
    - or a thin `Makefile` / `just` recipe wrapping `xelatex` + `biber`
5. Test-build once: `latexmk -xelatex main.tex` (or equivalent). First build may take noticeable time because `texlive.combined.scheme-full` is large.

CI is already scaffolded by the template — skip that offer in the "After Setup" step.

## Why `flake.lock` is checked in (and nowhere else)

TeX Live output is exquisitely sensitive to package versions — a single `tlpdb` update can silently change kerning, font metrics, or break `chktex` / `tex-fmt`. For most languages "track a stable channel" is good enough because the toolchain is semver-disciplined; TeX is not.

The template pins to `nixpkgs-unstable` (because TeX Live lands on unstable first) **and** ships a `flake.lock` so "unstable + reproducible" is coherent. The two together are load-bearing — dropping either one breaks the guarantee.

**Mirror rule**: the canonical `flake.lock` lives in `~/dev/tex/shigyo/`. If `shigyo` runs `nix flake update`, also update `furedea/template-tex`'s `flake.lock` so freshly-initialized TeX projects stay aligned.

## Why `scheme-full`

`texlive.combined.scheme-full` is the complete distribution — large (~5 GB) but avoids the "missing package" rabbit hole where every project adds one more `texlivePackages.*`. Disk is cheap compared to debugging "why does `\usepackage{foo}` fail on this machine". When a project has hard closure-size constraints (CI runners, containers) switch to a narrower scheme as a one-off exception.

## Common first-run checks

- `xelatex --version` should print a recent TeX Live version.
- `latexmk -xelatex main.tex` should produce `main.pdf` on a minimal `\documentclass{article}` source.
- `tex-fmt main.tex` and `chktex main.tex` should both run without "command not found".

## What NOT to do

- Do not drop `flake.lock` "to match the other templates". TeX is the exception for the reason above.
- Do not switch the TeX template from `nixpkgs-unstable` to a stable channel without also re-evaluating whether the lock still makes sense.
- Do not install TeX Live via `brew` / `tlmgr` on the side. That reintroduces two sources of truth for packages.
