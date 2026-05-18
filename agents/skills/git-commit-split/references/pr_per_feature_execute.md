# Phase 3 — Execute (`pr-per-feature` mode)

Read this once the user has approved the plan and the mode is `pr-per-feature`. The goal is to land each planned commit on **its own branch**, push it, and open a **draft PR** — repeating for every commit in the plan. Merging is out of scope; the user decides when and how to merge.

The strategy decided in the plan (`independent` vs `stack`) only changes the **cut point** for each branch and the **PR base** — everything else is identical.

For the hunk-level partial-apply technique used inside the loop, see `references/hunk_split.md`. For branch-name slug generation, see `scripts/branch_name.py`.

## Setup

```bash
git rev-parse HEAD > /tmp/pre-split-head             # safety net for `git reset --hard` recovery
git rev-parse --abbrev-ref HEAD > /tmp/pre-split-branch    # remember where we started
git fetch origin                                      # so the cut point isn't stale
git reset                                             # clear the index so we start clean
```

The pre-split HEAD undoes any commits this session creates (`git reset --hard "$(cat /tmp/pre-split-head)"`). The starting branch is recorded so the loop can return to it between branches in `independent` mode. `git fetch origin` matters here because the cut point should be the latest `origin/<base>`, not whatever the local `<base>` happens to be — otherwise the PR will include unrelated commits.

For untracked files that need hunk-level splitting, mark them intent-to-add up front so they show up in `git diff`:

```bash
git add -N <new-file>      # only for files needing hunk split
```

## Resolving branch slugs

For each commit in the plan, generate the branch slug from its subject line. The script avoids collisions with both local and remote refs in one shot:

```bash
slug=$(<skill-dir>/scripts/branch_name.py "<conventional message>" --avoid-existing)
```

Resolve `<skill-dir>` as in `references/hunk_split.md`. The script drops the optional commit scope and produces `<type>/<kebab-subject>`; if the slug already exists locally or on `origin`, it appends `-2`, `-3`, … and prints the unused name.

If the subject contains a Conventional Commits breaking-change marker (`feat!:`), the `!` is dropped from the slug — the type alone is enough. When passing such a subject through a shell, disable history expansion or single-quote it: `set +H; ...py 'feat!: subject'`.

## Per-commit loop

For each planned commit, in order. The branch creation in step 0 is what makes this mode different from `direct`; the rest mirrors the `direct` flow.

0. **Create the feature branch from the right cut point.** `git switch -c` preserves the uncommitted working tree, so the diff to be committed travels with you.
   ```bash
   # independent: every branch is cut from the latest origin/<base>
   git switch -c "$slug" "origin/<base>"

   # stack: branch 1 is cut from origin/<base>; branch n>1 is cut from <prev-branch>
   git switch -c "$slug" "<prev-branch>"
   ```
   After this step the working tree should still show every remaining hunk in `git status` — the branch creation only moves where the next commit will land.

1. **Whole-file commits** (most common path):
   ```bash
   git add -- <file1> <file2> ...
   git commit -m "<conventional message>"
   ```
   For deletions, `git add -- <deleted-file>` works too (stages the deletion).

2. **Hunk-level commits** (when one file is split across commits): see `references/hunk_split.md`. After applying the partial patch:
   ```bash
   git commit -m "<conventional message>"
   ```

3. **Mixed commit** (some whole files + some partial hunks): apply the partial patch first, then `git add` the whole-file additions, then commit once.

4. **Push the branch and open a draft PR:**
   ```bash
   git push -u origin "$slug"
   gh pr create -df --base "<pr-base>"
   ```
   - `-d` makes it a draft, `-f` fills the title and body from the commit. Drafts let reviewers know the work isn't yet ready and keep CI cheap until the user marks the PR ready.
   - `<pr-base>` is `<base>` for `independent`, and the previous branch's slug for `stack`.
   - If `gh pr create` fails (the repo doesn't allow drafts on this plan, the branch was just pushed and `gh` hasn't seen it, etc.), surface the error and pause — don't try clever recovery. The branch and commit are already safe; the user can run `gh pr create` themselves.

5. **Move to the next iteration's starting point.** The working tree may still hold remaining hunks for later commits.
   - **independent:** `git switch "$(cat /tmp/pre-split-branch)"` (or `git switch <base>` if the user started off the base) to return to the cut point. The remaining uncommitted changes travel with you. The next iteration's step 0 will cut a fresh branch from here.
   - **stack:** stay on the current branch — the next iteration's step 0 will cut its branch from here.

   Do not delete the just-pushed branch locally; the PR points at it and the user may need to amend.

## Post-commit verification

```bash
git status                                 # should be clean (or only have intentionally-left changes)
gh pr list --author @me --state open \
    --json number,title,headRefName,baseRefName,url --limit 20
```

Report each branch name, the PR number/URL, and the PR base. For `stack`, remind the user the merge order must follow the stack from bottom to top (PR 1 → PR 2 → …) so the bases keep pointing at landed commits.

If `git status` still shows pending changes when the plan claimed to cover everything, **stop and report** — don't paper over it. Likely causes: a hunk that didn't apply (line drift), an `.gitignore` masking a file, or a missed binary file.

## Recovery

The partial state at any failure point is *commits on local feature branches, possibly with PRs already opened*. Be conservative:

- **Branch created but commit failed.** Just retry the commit on the current branch. Nothing was pushed yet.
- **Commit landed but push failed.** The local commit is safe. Diagnose (network, permissions, branch protection on `<base>` accidentally targeted) and retry `git push -u origin "$slug"`.
- **Push succeeded but `gh pr create` failed.** The branch is on the remote with the commit; the user can open the PR manually. Tell them what to run: `gh pr create -df --base "<pr-base>"`.
- **You need to abort the whole session.** Use the safety net from setup, but only after confirming with the user — local commits and pushed branches will be lost on the local side, and pushed branches stay on the remote until someone deletes them:
  ```bash
  git switch "$(cat /tmp/pre-split-branch)"
  git reset --hard "$(cat /tmp/pre-split-head)"
  ```
  Pushed branches and any draft PRs already opened are not auto-cleaned. Tell the user which branches were pushed and let them decide whether to `git push origin --delete <slug>` and close the PRs, or keep them.

## Edge cases specific to `pr-per-feature` mode

- **Pre-commit hook failure.** Same handling as `direct`: fix and create a new commit, never `--amend`. The branch's first push hasn't happened yet, so retry the commit before pushing.
- **Push rejected on the base branch.** This shouldn't happen in normal flow — every push goes to a feature slug, never to `<base>`. If it does, stop immediately. Never `--force` past a protection rule; the protected base is exactly *why* this mode exists.
- **`gh` not authenticated / no remote.** Phase 0 catches this, but if it surfaces mid-execution (token expired, network blip), pause and tell the user. The local commit is intact; they can re-auth and resume by running `git push -u origin "$slug"` and `gh pr create -df --base "<pr-base>"` themselves.
- **Branch name collision discovered after the slug was generated.** `branch_name.py --avoid-existing` checks at slug-generation time, but a parallel session could create a colliding branch in the meantime. If `git switch -c` fails locally, regenerate the slug with `--avoid-existing` and update the plan. If `git push` is rejected as `(fetch first)`, fetch and pick a new slug — never `--force` over someone else's branch.
- **Stack base lags after a previous PR is merged.** Out of scope for this skill — it doesn't merge or rebase stacks. If the user merges PR 1 themselves while later PRs are still open, those PRs' bases will rebase on the next push; that's the user's call, not this skill's.
- **Detached HEAD.** Hard stop. This mode requires a named branch to push from.
- **One giant change that genuinely is one feature.** This still means one branch + one PR — the mode doesn't manufacture extra branches. Land the single commit, push, open one draft PR, done.
