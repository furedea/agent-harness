---
name: git-commit-split
description: >
    Explicit `/git-commit-split` workflow for organizing an already-dirty working tree into multiple intent-grained commits, optionally pairing each commit with its own branch and PR. Use only when invoked as the custom command or when the user explicitly asks to split pending changes, create one feature per commit, or create one PR per feature. General branch naming, commit granularity, and Conventional Commits rules live in git-workflow; this command owns mode selection, dirty-tree inspection, hunk splitting, plan approval, and execution.
---

Mode argument from slash-command invocation (empty if none was passed): $ARGUMENTS

# `/git-commit-split` custom command

This command is for a dirty working tree that already contains multiple uncommitted intents. It inspects everything pending, proposes a split plan, waits for approval, then lands one commit per approved intent. Optionally each commit can be packaged into its own branch and PR.

Day-to-day implementation workflow belongs to `git-workflow`. Do not use this command as the default answer to a normal "implement/fix/refactor" request.

The work happens in four phases: **mode → inspect → plan → execute**. This file owns mode/inspect/plan and routes execute to a mode-specific reference. Show the plan and wait for explicit user approval before any branch or commit lands.

## Files in this command

- `references/hunk_split.md`: zero-context partial-apply technique — read whenever a commit splits one file's hunks across commits, in either mode.
- `references/direct_execute.md`: Phase 3 for `direct` mode.
- `references/pr_per_feature_execute.md`: Phase 3 for `pr-per-feature` mode (covers both `independent` and `stack`).
- `scripts/build_partial_patch.py`: filter a unified diff to a subset of hunks.
- `scripts/branch_name.py`: generate a kebab-case branch slug from a Conventional Commits subject, with optional collision avoidance against local + `origin`.

## Phase 0 — Mode selection

The first decision is _where_ the commits will live. There are exactly two delivery modes:

| mode | meaning |
| --- | --- |
| `direct` | Commit on the current branch. No branch creation, no push, no PR. Best when you're already on a feature branch, or when the repo has no protections on the current branch. |
| `pr-per-feature` | Create one branch + one PR per commit. Within this mode, the **branching strategy** is a separate sub-decision presented in the plan: `independent` (each branch cut from the base, fully parallel PRs) or `stack` (each branch cut from the previous feature branch, dependent PRs). |

**The mode is always set explicitly by the user. Do not auto-detect it.** Auto-detection (e.g., probing `gh api .../branches/main/protection`) is unreliable across environments, silently picks the wrong workflow when authentication is missing, and can push to a protected branch by accident. The cost of asking is one short message; the cost of guessing wrong is a force-push, a denied push that confuses the user, or an unintended PR.

Resolve the mode in this order:

1. If this command surfaced a non-empty slash-command argument and it equals `direct` or `pr-per-feature`, use that as the mode. Sub-strategy (`independent` / `stack`) is **not** taken from the argument — keep it for the dialog in step 3. If the argument is something else (typo, unrelated text), ignore it and fall through.
2. Otherwise, if the user's prompt explicitly names the mode (e.g., "PR に分けて", "branch 切って PR", "1 機能 1PR", "stack PR" → `pr-per-feature`; "main に直接", "ここで commit", "この branch に commit" → `direct`), use that.
3. Otherwise ask **one** short question and wait. Example phrasing:

    > この commit 分割は (a) 現在の branch に直接 commit する `direct` モードと，(b) 1 機能ごとに branch を切って PR を出す `pr-per-feature` モード のどちらで進めますか？ `pr-per-feature` の場合は，各 PR を独立に base から切る `independent` か，順番に積む `stack` かも併せて教えてください（迷う場合は `independent` が無難です）．

    When step 1 already settled the mode as `pr-per-feature`, ask only the sub-strategy half (`independent` / `stack`) instead of repeating the mode question.

4. Do not proceed until the user answers (or step 1 already resolved everything needed). Treat silence/ambiguity as "ask again", not "default to direct".

For `pr-per-feature`, also confirm prerequisites _before_ inspecting:

- `gh auth status` — `gh` must be authenticated. If `gh` is missing entirely (e.g., a Claude.ai sandbox or a minimal container) or unauthenticated, surface the failure and stop. Don't fall back to `direct` silently — the user picked `pr-per-feature` for a reason. Either ask them to authenticate / install `gh`, or get explicit confirmation to switch the mode.
- `git remote -v` — there must be a push remote (typically `origin`) that points to a host `gh` understands. If absent, stop and report.
- Identify the **base branch** (default branch of the remote) with `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`. This is the PR target for `independent`, and the cut point of the _first_ branch in `stack`. Record it as `<base>`.

## Phase 1 — Inspect

Read every pending change before grouping. Skipping this leads to surface-level groupings ("commit each file") that miss the intent.

```bash
git rev-parse --is-inside-work-tree   # bail early if not in a repo
git rev-parse --abbrev-ref HEAD       # note current branch
git status --porcelain=v1             # tracked + untracked, machine-readable
git diff                              # unstaged tracked changes
git diff --staged                     # already-staged changes
```

For each file in `git status`:

- **Modified (` M` / `M `)** — read the diff hunks.
- **Untracked (`??`)** — read the file content with `Read`. If it's small and clearly one feature, plan to add it whole; if it's large and mixes concerns, run `git add -N <file>` so it appears in `git diff` and can be hunk-split.
- **Deleted (` D` / `D `)** — note as a deletion; usually pairs with whatever feature removed it.
- **Renamed/Copied (`R`/`C`)** — keep the rename atomic (don't split a rename across commits).

Do not summarize prematurely. Read the actual changes — names, signatures, behavior — so the grouping reflects what the code does, not what the filenames suggest.

## Phase 2 — Plan

Group hunks into commits by **intent**, not by file. Use `git-workflow` for the shared commit granularity, Conventional Commits, and branch naming rules. This command adds the dirty-tree-specific planning details: which hunks/files belong to each commit, which mode will execute them, and what branch/PR shape will be created in `pr-per-feature`.

### How to group

- Apply `git-workflow`'s commit granularity rules first.
- When one file contains multiple intents, split it by hunk; see `references/hunk_split.md`.
- Keep generated files, lockfiles, and tests with the intent that caused them unless the existing repo history clearly uses a different convention.
- Do not fabricate splits when the dirty tree is genuinely one cohesive change.

### Branch / PR plan (only for `pr-per-feature`)

When the mode is `pr-per-feature`, the commit grouping is only half the plan. The user also needs to see and approve:

- **Branch strategy** — `independent` or `stack`. Recommend `independent` unless the commits build on each other in a way the reviewer needs to follow in order (e.g., commit 2 is a refactor that commit 3 depends on). When in doubt, propose `independent` and explain that `stack` is available if dependencies matter.
- **Branch names** — generated by `scripts/branch_name.py` from each commit subject, following `git-workflow`'s branch format. If a name already exists locally or remotely, the script appends `-2`, `-3`, …; surface any collision in the plan.
- **PR base** — for `independent`, every PR targets `<base>` (the remote default branch). For `stack`, PR `n` targets the branch from PR `n-1`; PR 1 targets `<base>`.
- **PR shape** — every PR is created as a normal PR via `gh pr create -f --base <base>` (`-f` = fill title/body from the commit).

### Plan presentation

Show the plan as a numbered list, in the order the commits will land. For each commit include the message, the affected files, and — for hunk-split commits — the line ranges or a one-line summary of which hunks. For `pr-per-feature`, also include the branch name and PR base for each entry, and state the branch strategy and base branch up front. Then ask for approval.

**`direct` example:**

```
Mode: direct (commits land on current branch `feature/big-batch`)

Proposed commits (3):

1. feat(auth): add JWT refresh-token rotation
   - src/auth/refresh.ts (new file)
   - src/auth/login.ts (hunks 1-2: wire refresh into login)
   - tests/auth/refresh.test.ts (new file)

2. fix(parser): handle empty input without panicking
   - src/parser/index.ts (hunk 3 only)

3. docs: note refresh-token flow in README
   - README.md

Apply this plan? (yes / edit / cancel)
```

**`pr-per-feature` example:**

```
Mode: pr-per-feature
Strategy: independent  (each PR cut from `main`, no dependencies)
Base branch: main
PR shape: normal (gh pr create -f)

Proposed commits / branches (3):

1. feat(auth): add JWT refresh-token rotation
   branch: feat/jwt-refresh-rotation   →  PR base: main
   - src/auth/refresh.ts (new file)
   - src/auth/login.ts (hunks 1-2)
   - tests/auth/refresh.test.ts (new file)

2. fix(parser): handle empty input without panicking
   branch: fix/parser-empty-input      →  PR base: main
   - src/parser/index.ts (hunk 3 only)

3. docs: note refresh-token flow in README
   branch: docs/refresh-token-readme   →  PR base: main
   - README.md

Apply this plan? (yes / edit / cancel)
```

For `stack`, replace the strategy line and the per-entry PR base accordingly:

```
Strategy: stack  (each PR depends on the previous; merge in order)

1. ...   branch: refactor/extract-query-builder  →  PR base: main
2. ...   branch: feat/repository-cache           →  PR base: refactor/extract-query-builder
3. ...   branch: test/repository-cache-edges     →  PR base: feat/repository-cache
```

If the user requests edits, revise and re-present — never silently change the plan. Treat branch-strategy changes (independent ↔ stack) as a full re-plan; a stack and a parallel set are very different review experiences.

## Phase 3 — Execute (router)

Once the user approves, switch to the reference for the chosen mode and follow it end-to-end:

- `direct` → read `references/direct_execute.md`
- `pr-per-feature` → read `references/pr_per_feature_execute.md`

Both modes share the hunk-level partial-apply technique in `references/hunk_split.md`; load it once when the loop first hits a hunk-split commit.

The key invariant for both modes: **after each commit, regenerate the diff against the new HEAD before building the next partial patch**, because line numbers shift as commits land. The mode-specific references repeat this where it matters.

## Edge cases (mode-independent)

These apply regardless of mode. Mode-specific edge cases live in each `*_execute.md`.

- **Nothing to commit.** If `git status` is clean before starting, say so and stop. Don't invent commits.
- **Merge in progress** (`.git/MERGE_HEAD` exists). Don't try to split — the user is mid-merge. Surface this and ask.
- **Submodule pointer changes.** Treat as a single file; usually goes in a `chore(submodule):` or with the feature that bumped it.
- **Binary files.** No hunk-level split possible — commit whole or skip.
- **User commits in non-English.** Follow the language rule in `git-workflow`. Branch names stay ASCII because some hosts and tools choke on non-ASCII refs.

## Navigation map (read-on-demand)

- Phase 3 (execute), `direct` mode: `references/direct_execute.md`
- Phase 3 (execute), `pr-per-feature` mode (independent or stack): `references/pr_per_feature_execute.md`
- Hunk-level partial apply (any mode, when one file's hunks split across commits): `references/hunk_split.md`
- Generate a kebab-case branch slug from a commit subject (with collision avoidance): `scripts/branch_name.py`
- Filter a unified diff to a subset of hunks: `scripts/build_partial_patch.py`
