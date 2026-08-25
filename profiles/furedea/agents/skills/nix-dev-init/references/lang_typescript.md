# Phase 2: TypeScript / Node (pnpm)

Prerequisite: repo created via `cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-typescript)"`. The template provides `flake.nix`, `.envrc`, `package.json`, `tsconfig.json`, `.oxlintrc.json`, `.oxfmtrc.json`, `vitest.config.ts`, `pnpm-workspace.yaml`, `lefthook.yml`, `.commitlintrc.yml`, CI workflows (lint, format, typecheck, test, CodeQL), and `.gitignore`. `github/create_repo.sh` also patches `package.json`'s `name` field and applies GitHub rulesets.

## Steps

1. `direnv allow` — the template already includes `.envrc` (`use flake`).
2. Verify `which node` and `which pnpm` resolve under `/nix/store/`.
3. `pnpm install` — resolves dependencies and creates `node_modules/` + `pnpm-lock.yaml`.
4. Hand off to relevant downstream conventions if applicable.

CI is already scaffolded by the template — skip that offer in the "After Setup" step.

## Why pnpm instead of npm / yarn

pnpm's content-addressable store (`~/.local/share/pnpm/store/`) pairs naturally with nix's store model: both deduplicate by hash, both avoid the "works on my machine" problem that flat `node_modules` causes. The user's dotfiles standardize on pnpm, so staying with it means shared cache across projects.

## Why oxlint / oxfmt instead of eslint / prettier

They are Rust-based, orders of magnitude faster than the JS-native equivalents, and the template already includes their configs. Do not swap them out for eslint / prettier unless the project has a concrete reason (e.g., a shared config from upstream).

## Common first-run checks

- `node --version` should print `v22.x` and resolve under `/nix/store/`.
- `pnpm install` should succeed without network access to a non-pnpm registry.
- `pnpm run lint` and `pnpm run format:check` should run cleanly.

## What NOT to do

- Do not run `pnpm init` — the template repo already provides `package.json`. Running `pnpm init` overwrites it and loses the curated config.
- Do not add `nodejs` or `pnpm` to `~/ghq/github.com/furedea/dotfiles/nix/home/default.nix`. Project-level pinning is the whole point of Phase 1.
- Do not commit `node_modules/`. pnpm's lockfile + nix's `nodejs_22` pin is what makes the build reproducible.
- Do not mix package managers (npm install + pnpm install in the same repo). pnpm's lockfile format is not interchangeable with npm's.
