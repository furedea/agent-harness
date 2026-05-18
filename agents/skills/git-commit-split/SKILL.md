---
name: git-commit-split
description: >
    Split pending git changes (modified, untracked, and already-staged) into multiple feature-grained commits with Conventional Commits messages, optionally pairing each commit with its own branch and draft PR. Use whenever the user asks to "commit", "コミットして", "make commits", "split into commits", "1 機能 1commit", "1 コミット 1 機能", "commit unstaged files", "未ステージを commit", "branch 切って PR", "PR 分けて", "1 機能 1PR", "stack PR", "draft PR", "ドラフト PR", or any phrasing that implies organizing pending git changes into well-structured commits. Supports two modes: `direct` (commit on the current branch) and `pr-per-feature` (one branch + one draft PR per commit, independent or stacked). The user must set the mode explicitly; never auto-detect it. Present commit grouping and branch/PR strategy as one plan for approval before any branch or commit lands. Prevent vague "update files" commits, direct pushes to protected main, and missing Conventional Commits formatting.
---

Mode argument from slash-command invocation (empty if none was passed): $ARGUMENTS

# Git commit split (one feature, one commit)

A working tree often accumulates several unrelated changes before anyone gets around to committing — a feature, a fix, a config tweak, some stray TODO. Squashing all of that into one commit destroys the history's usefulness for `git blame`, review, revert, and bisect. The goal of this skill is to look at everything pending, group changes by intent, and emit one Conventional Commits commit per intent — going down to hunk granularity when a single file mixes intents. Optionally each commit can be packaged into its own branch and draft PR.

The work happens in four phases: **mode → inspect → plan → execute**. This file owns mode/inspect/plan and routes execute to a mode-specific reference. Show the plan (commits _and_, in `pr-per-feature`, the branching/PR strategy) and wait for explicit user approval before any branch or commit lands; commits are easy to write and painful to undo, branch pushes and PRs are visible to others, and the small cost of a confirmation step pays for itself.

## Files in this skill

- `references/conventional_commits.md`: type table, scope rule, subject style, body — read in Phase 2.
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
| `pr-per-feature` | Create one branch + one draft PR per commit. Within this mode, the **branching strategy** is a separate sub-decision presented in the plan: `independent` (each branch cut from the base, fully parallel PRs) or `stack` (each branch cut from the previous feature branch, dependent PRs). |

**The mode is always set explicitly by the user. Do not auto-detect it.** Auto-detection (e.g., probing `gh api .../branches/main/protection`) is unreliable across environments, silently picks the wrong workflow when authentication is missing, and can push to a protected branch by accident. The cost of asking is one short message; the cost of guessing wrong is a force-push, a denied push that confuses the user, or an unintended PR.

Resolve the mode in this order:

1. If `SKILL.md` surfaced a non-empty slash-command argument and it equals `direct` or `pr-per-feature`, use that as the mode. Sub-strategy (`independent` / `stack`) is **not** taken from the argument — keep it for the dialog in step 3. If the argument is something else (typo, unrelated text), ignore it and fall through.
2. Otherwise, if the user's prompt explicitly names the mode (e.g., "PR に分けて", "branch 切って PR", "1 機能 1PR", "draft PR", "stack PR" → `pr-per-feature`; "main に直接", "ここで commit", "この branch に commit" → `direct`), use that.
3. Otherwise ask **one** short question and wait. Example phrasing:

    > この commit 分割は (a) 現在の branch に直接 commit する `direct` モードと，(b) 1 機能ごとに branch を切って draft PR を出す `pr-per-feature` モード のどちらで進めますか？ `pr-per-feature` の場合は，各 PR を独立に base から切る `independent` か，順番に積む `stack` かも併せて教えてください（迷う場合は `independent` が無難です）．

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

Group hunks into commits by **intent**, not by file. A single commit should answer one question: _what user-visible thing changed, and why?_ For type/scope/subject conventions used in this phase, read `references/conventional_commits.md`.

### How to group

- Same feature, multiple files → one commit.
- Same file, multiple features → multiple commits via hunk split (see `references/hunk_split.md`).
- Pure formatting/whitespace mixed in with logic → split into a separate `style:` commit so the logic commit stays reviewable.
- Generated/lockfile updates (`package-lock.json`, `pnpm-lock.yaml`, `uv.lock`, …) belong with the change that caused them, not in their own commit.
- A test for feature X lives in the same commit as feature X (TSDD-friendly), unless the user has a different convention visible in `git log`.

### Branch / PR plan (only for `pr-per-feature`)

When the mode is `pr-per-feature`, the commit grouping is only half the plan. The user also needs to see and approve:

- **Branch strategy** — `independent` or `stack`. Recommend `independent` unless the commits build on each other in a way the reviewer needs to follow in order (e.g., commit 2 is a refactor that commit 3 depends on). When in doubt, propose `independent` and explain that `stack` is available if dependencies matter.
- **Branch names** — derived from each commit subject by `scripts/branch_name.py`. Format: `<type>/<kebab-subject>`, e.g., `feat/jwt-refresh-rotation`, `fix/parser-empty-input`. Lowercase ASCII, no scope. If a name already exists locally or remotely, the script appends `-2`, `-3`, …; surface any collision in the plan.
- **PR base** — for `independent`, every PR targets `<base>` (the remote default branch). For `stack`, PR `n` targets the branch from PR `n-1`; PR 1 targets `<base>`.
- **PR shape** — every PR is created as a **draft** via `gh pr create -df` (`-d` = draft, `-f` = fill title/body from the commit). The user can promote drafts later.

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
PR shape: draft (gh pr create -df)

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
- **One giant change that genuinely is one feature.** Don't fabricate splits. One commit is fine if the work is one cohesive thing; report that and commit once.
- **User commits in non-English.** See `references/conventional_commits.md` for language detection. Branch names stay ASCII regardless, since some hosts and tools choke on non-ASCII refs.

## Navigation map (read-on-demand)

- Phase 2 (plan) — Conventional Commits type/scope/subject style: `references/conventional_commits.md`
- Phase 3 (execute), `direct` mode: `references/direct_execute.md`
- Phase 3 (execute), `pr-per-feature` mode (independent or stack): `references/pr_per_feature_execute.md`
- Hunk-level partial apply (any mode, when one file's hunks split across commits): `references/hunk_split.md`
- Generate a kebab-case branch slug from a commit subject (with collision avoidance): `scripts/branch_name.py`
- Filter a unified diff to a subset of hunks: `scripts/build_partial_patch.py`
