# Phase 2: Python (uv)

Prerequisite: repo created via `cd "$($DOTFILES/github/create_repo.sh <name> --private --template furedea/template-python)"`. The template provides `flake.nix`, `.envrc`, `pyproject.toml`, CI workflows, `.gitignore`, `lefthook.yml`, `.commitlintrc.yml`, and `renovate.json`. `github/create_repo.sh` also patches `pyproject.toml`'s `name` field and applies GitHub rulesets.

## Steps

1. `direnv allow` — the template already includes `.envrc` (`use flake`).
2. Verify `which uv` and `which python3` resolve under `/nix/store/`.
3. Create `src/<package_name>/` and `tests/` directories.
4. `uv sync` — resolves dependencies and creates `.venv/`.
5. Hand off to the `python-style` skill.

CI (`github-ci-init`) is already scaffolded by the template — skip that offer in the "After Setup" step.

## Why uv is pinned to the nix interpreter

The template's `flake.nix` sets `UV_PYTHON_DOWNLOADS=never` + `UV_PYTHON_PREFERENCE=only-system` so uv cannot silently download a `python-build-standalone` binary into `~/.local/share/uv/python/`. Without that, you end up with two interpreters on one machine — nix's and uv's — and neither is reproducible from the other. With it, **nix is the single source of truth** for the interpreter and uv is reduced to resolver + lockfile + venv.

If nix's `python3` is too old for `requires-python`, uv fails loudly. That is the correct failure mode — fix it by bumping nixpkgs, not by unsetting the env vars.

## Common first-run checks

- `uv sync` succeeds **without downloading a Python** — this is the only direct confirmation that `UV_PYTHON_DOWNLOADS=never` is in effect.
- `python -c "import sys; print(sys.executable)"` prints a path under `.venv/` whose interpreter symlinks back to `/nix/store/...-python3-*`.

## What NOT to do

- Do not run `uv init` — the template repo already provides `pyproject.toml`. Running `uv init` overwrites it and loses the curated tool config.
- Do not unset `UV_PYTHON_DOWNLOADS` / `UV_PYTHON_PREFERENCE` to "just make it work". Those env vars are load-bearing; removing them silently reintroduces the two-interpreter problem this skill exists to prevent.
- Do not commit `.venv/` — it is a cache keyed to the nix store path and will rot on any flake update.
