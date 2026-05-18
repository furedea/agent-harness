# Phase 3 — Execute (`direct` mode)

Read this once the user has approved the plan and the mode is `direct`. The goal is to land each planned commit on the **current branch**, in order, without creating new branches, pushing, or opening PRs.

The key invariant: **after each commit, regenerate the diff against the new HEAD before building the next partial patch**, because line numbers shift as commits land.

For the hunk-level partial-apply technique used inside the loop, see `references/hunk_split.md`. The mechanics are the same as in `pr-per-feature`; only the surrounding flow differs.

## Setup

```bash
git rev-parse HEAD > /tmp/pre-split-head            # safety net for `git reset --hard` recovery
git rev-parse --abbrev-ref HEAD > /tmp/pre-split-branch   # informational; useful in error reports
git reset                                            # clear the index so we start clean
```

The pre-split HEAD is the actual safety net: if anything goes wrong mid-execution, `git reset --hard "$(cat /tmp/pre-split-head)"` undoes every commit this session created. The working tree is preserved by the plain `git reset` (no `--hard`), which only touches the index.

For untracked files that need hunk-level splitting, mark them intent-to-add up front so they show up in `git diff`:

```bash
git add -N <new-file>      # only for files needing hunk split
```

## Per-commit loop

For each planned commit, in order:

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

## Post-commit verification

```bash
git status                                 # should be clean (or only have intentionally-left changes)
git log --oneline -n <N>                   # confirm N commits landed in order
```

If `git status` still shows pending changes when the plan claimed to cover everything, **stop and report** — don't paper over it. Likely causes: a hunk that didn't apply (line drift), an `.gitignore` masking a file, or a missed binary file.

## Recovery

If commits already landed and need undoing:

```bash
git reset --hard "$(cat /tmp/pre-split-head)"   # destructive; confirm with user first
```

Prefer `git reset --soft "$(cat /tmp/pre-split-head)"` if you want to rewrite the commits rather than discard the work — that keeps every change staged so you can recommit.

## Edge cases specific to `direct` mode

- **Pre-commit hook failure.** A failed hook means the commit didn't land. Read the hook output, fix the issue (typically a lint/format problem), `git add` the fix, and **create a new commit** — never `--amend` an attempted-but-failed commit, because the previous successful commit would be modified instead.
- **Detached HEAD.** Warn the user — committing on detached HEAD risks orphaning the work. Suggest `git switch -c <branch>` first.
- **Pushing.** This skill in `direct` mode never pushes. The user pushes on their own when they're ready. If the user wants automated push + PR, the right answer is `pr-per-feature` mode, not patching push into `direct`.
