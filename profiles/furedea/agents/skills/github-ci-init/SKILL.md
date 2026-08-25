---
name: github-ci-init
description: >
    GitHub Actions CI scaffolding for this user. Installs the curated "default adopted set" — release-please, Claude Code Action (@claude mentions + automated PR review) — by copying bundled workflow templates into a repository's `.github/`. Load whenever the user wants to add CI, GitHub Actions, automated quality checks, release-please, Claude Code Action, automated code review, or retrofit CI onto an existing repository. This skill is DISTINCT from `gha-style` (which covers the coding conventions for writing and editing individual workflow files) — load THIS one for the initial scaffolding (the "which workflows ship and where do they go" decision), then hand off to `gha-style` for any per-file editing afterwards. Also trigger on "/github-ci-init", "set up CI", "add GitHub Actions", "release-please", "claude code action", "claude code review", "@claude workflow", "artifact attestation", "build provenance", "SLSA", ".github/workflows/", or any mention of project-level CI bootstrap.
---

# GitHub CI Init Workflow

## Scope

This skill adds **optional, project-specific** GitHub Actions CI workflows to a repository. It complements the `furedea/template-minimal` GitHub template, which already ships the baseline workflows (`gha_lint.yml`, `dependency_review.yml`, `dependency_review_config.yml`, `renovate.json`).

It does NOT own **writing or editing the YAML contents of individual workflow files** — that belongs to the `gha-style` skill (permissions minimization, action version pinning, script injection prevention, etc.).

## When to apply vs skip

Not every project benefits from these optional workflows. Applying them to a throwaway repo creates maintenance noise (CodeQL alerts on every PR, release-please commits) that outweighs the payoff.

### Apply when

- The project has a plausible long-term lifetime (weeks to years)
- PRs are expected (from others, or from yourself via feature branches)
- The project is public, or likely to become public

### Skip when

- It's a throwaway experiment, tutorial follow-along, or one-off script
- The project lives in a single commit and won't be touched again
- It's a scratch space for exploring a library

When in doubt, **skip**. CI can always be added later by running this skill again on the existing repo — that is exactly the retrofit case it is designed for.

## What templates already provide

These files are already in GitHub templates and should NOT be added by this skill:

- `gha_lint.yml` — actionlint + zizmor (in `template-minimal` and all derived templates)
- `dependency_review.yml` + `dependency_review_config.yml` — dependency vulnerability review (same)
- `renovate.json` — automated dependency updates (same)
- `codeql.yml` — CodeQL analysis (in language-specific templates: `template-rust`, `template-python`, `template-typescript`)

## Steps

1. **Confirm the project passes the "apply" criteria**. If it does not, stop and explain why CI would be net-negative for this project.
2. **Create `.github/workflows/`** in the project root if it does not already exist.
3. **Copy the relevant workflow files** from `templates/` (in this skill's directory) into the project's `.github/workflows/`:
   - `release_please.yml` — opens release PRs from Conventional Commits; ship when the project will have versioned releases. **Replace `release-type: simple`** with a repo-specific strategy (e.g. `python`, `node`, `rust`) if needed.
   - `claude.yml` — responds to `@claude` mentions in issues, PRs, and review comments via Claude Code Action (OAuth). Requires the `CLAUDE_CODE_OAUTH_TOKEN` repository secret.
   - `claude_code_review.yml` — runs automated Claude Code Review on every PR via the `code-review` plugin. Requires the same `CLAUDE_CODE_OAUTH_TOKEN` secret.
   - `artifact_attestation.yml` — generates SLSA build provenance attestations on release via Sigstore. Ship when the project publishes binaries, packages, or container images. **Replace the build steps and `subject-path`** to match the project's actual build outputs.
4. **Remind the user** about the GitHub-side features (Push Protection / Secret Scanning / Rulesets). These live in repo or org settings, not in `.github/workflows/`, so this skill cannot install them.

## Handoff to `gha-style`

After copying, any edit to the resulting `.github/workflows/*.yml` — changing triggers, tuning `permissions:`, adding a job, pinning an action version, fixing an `actionlint` warning, responding to a `zizmor` finding — belongs to the `gha-style` skill.

This split is deliberate:

- `github-ci-init` owns the **"which files exist"** decision. It is a one-shot, per-repo scaffolding action.
- `gha-style` owns the **"how each file is written"** decision. It is an ongoing code-review loop every time a workflow is touched.

Duplicating either side into the other would let the two drift. Keep them separate and chain them: scaffold with this skill, then edit with `gha-style`.

## Anti-patterns

- **Applying the full set to a throwaway experiment**. CodeQL/release-please on a 1-commit repo is pure maintenance noise.
- **Editing workflow YAML inside this skill's flow**. Redirect to `gha-style`. Even "just tuning one permission" should go through that skill so the conventions stay consistent.
- **Deploying Claude Code Action without the `CLAUDE_CODE_OAUTH_TOKEN` secret** configured in the repository. The workflows will fail silently on every trigger.
- **Re-adding workflows that templates already provide** (gha_lint, dependency_review, renovate, codeql). These are maintained in the GitHub templates — duplicating them here causes drift.
