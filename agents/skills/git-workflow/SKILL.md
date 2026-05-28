---
name: git-workflow
description: >
    Day-to-day Git workflow for code changes, branches, commits, and delivery safety. Use before code changes or branch/commit work, or when the user says "実装", "修正", "直して", "リファクタ", "追加", "更新", "ブランチ", "branch", "コミット", or "commit". Establishes feature-branch naming, Conventional Commits, one-intent commits, TSDD-friendly commit boundaries, and safe push behavior.
---

# Git Workflow

This skill governs the default Git shape of implementation work: which branch to use, how to name it, how to cut commits, and when it is safe to push or open a PR. It is intentionally lighter than the `/git-commit-split` custom command, which is for taking an already-dirty working tree and splitting it into commits or PRs.

## Operating Rules

- Inspect Git state before edits: current branch, `git status --porcelain=v1`, and recent commit style when commit messages will be written.
- Never overwrite, reset, clean, or discard user changes unless the user explicitly asked for that exact destructive action.
- Do not push, force-push, merge PRs, or open PRs unless the user explicitly asked for that delivery step.
- Prefer one coherent VCS unit per Red -> Green -> Refactor cycle. If the task is too small for multiple cycles, one commit is enough.
- Keep branch names and commit subjects aligned with the primary intent of the change, not with filenames.

## Branch Policy

Use a feature branch for implementation work.

1. If already on a suitable non-default branch, stay there.
2. If on `main`, `master`, a release branch, or another protected/default branch, create a task branch before edits unless the user explicitly asked to work on the current branch.
3. If the working tree is dirty before branch creation, inspect the dirty files. If they are unrelated user changes, ask before switching or committing them.
4. If the user asks for a PR, create the branch before implementation and keep all commits for that PR on the branch.

Branch name format:

```text
<type>/<kebab-subject>
```

Rules:

- `<type>` is the Conventional Commits type that would be used for the primary commit: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`, `style`, or `revert`.
- `<kebab-subject>` is lowercase ASCII, hyphen-separated, derived from the imperative commit subject with no scope.
- Omit Conventional Commits scope from branch names.
- Append `-2`, `-3`, etc. if a local or remote branch already exists.

Examples:

```text
feat/jwt-refresh-token-rotation
fix/parser-empty-input
refactor/extract-query-builder
docs/clarify-install-steps
```

## Commit Granularity

Commit by intent, not by file.

- One user-visible feature, bug fix, refactor, documentation update, or config change per commit.
- Tests for a new behavior belong in the same commit as the implementation.
- Test-only coverage for existing behavior uses `test:`.
- Generated files and lockfiles belong with the change that caused them.
- Pure formatting belongs in `style:` when it would obscure a logic review.
- Do not fabricate splits. One cohesive change should be one commit.
- If one file contains multiple unrelated intents, split hunks or use the `/git-commit-split` custom command when the task is specifically to organize pending changes.

Before committing, verify the relevant test or quality gate is green. If the full suite is too expensive or unrelated failures exist, run the narrowest gate that proves the change and report the limitation.

## Conventional Commits

Use Conventional Commits for every commit message.

```text
<type>(<scope>): <imperative subject>
```

Scope is optional:

- Use scope when all changed files clearly live in one module or area.
- Omit scope when the commit spans multiple top-level areas or repo-root files.
- Keep scope lowercase, single-word, and without slashes.

Subject rules:

- Imperative mood: `add`, `fix`, `remove`, `rename`.
- Lowercase first letter unless it is a proper noun or identifier.
- No trailing period.
- Aim for 50 characters or fewer; hard cap at 72.
- Describe behavior or intent, not filenames.

Use a body only when the why is not obvious from the subject. Wrap body text at about 72 columns.

Common types:

| type       | when to use                                      |
| ---------- | ------------------------------------------------ |
| `feat`     | new user-visible capability                      |
| `fix`      | bug fix                                          |
| `refactor` | restructure without behavior change              |
| `perf`     | performance-only change                          |
| `docs`     | documentation only                               |
| `test`     | test-only change for existing behavior           |
| `build`    | build system, packaging, dependencies, lockfiles |
| `ci`       | CI configuration only                            |
| `chore`    | tooling/config that does not fit elsewhere       |
| `style`    | formatting only, no logic change                 |
| `revert`   | reverts a previous commit                        |

Examples:

```text
feat(auth): add JWT refresh-token rotation
fix(parser): handle empty input without panicking
refactor(db): extract query builder from repository
docs: clarify install steps for Apple Silicon
test(auth): cover refresh-token expiry edge case
build(deps): bump axios from 1.6.0 to 1.7.2
```

## Delivery Flow

For implementation tasks:

1. Inspect branch and dirty state.
2. Move to or create the feature branch when branch policy requires it.
3. Implement with TSDD: Red -> Green -> Refactor.
4. Commit each coherent green unit when the user asked for commits or the repository workflow expects implementation work to be committed.
5. Stop before push/PR unless the user explicitly requested that delivery.

For explicit commit requests:

1. Inspect all pending changes before grouping.
2. If changes are already mixed across multiple intents, switch to the `/git-commit-split` custom command instead of improvising partial commits here.
3. Otherwise commit one coherent intent with a Conventional Commits message.

For explicit PR requests:

1. Confirm the target base branch if it is not obvious from the repo default.
2. Push the feature branch only after the user requested PR delivery.
3. Create a draft PR unless the user explicitly asks for a ready-for-review PR.

## Relationship To `/git-commit-split`

Use this skill for normal work as it is being implemented.

Use the `/git-commit-split` custom command when the user's task is specifically to organize existing pending changes into multiple commits, multiple branches, or one PR per feature. Do not duplicate its hunk-splitting and PR-per-feature execution flow here.
