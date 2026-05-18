# Hunk-level partial apply

Read this when a single file mixes multiple intents and a commit needs to land only some of its hunks. The procedure is the same in `direct` and `pr-per-feature` mode — only the surrounding flow (branch creation, push, PR) differs.

## Build a partial patch and apply it to the index

Resolve `<skill-dir>` to the directory containing `SKILL.md` — typically `~/.claude/skills/git-commit-split` (Claude Code) or `~/.codex/skills/git-commit-split` (Codex). Both point at the same files via dotfiles symlinks, so either form works.

```bash
git diff -U0 --src-prefix=a/ --dst-prefix=b/ -- <file> > /tmp/cur.patch
<skill-dir>/scripts/build_partial_patch.py \
    /tmp/cur.patch '[{"file": "<file>", "hunks": [1, 3]}]' > /tmp/partial.patch
git apply --cached --unidiff-zero /tmp/partial.patch
git commit -m "<conventional message>"
```

## Why these flags

- **`-U0`** produces zero-context hunks. The default `-U3` merges adjacent edits within 3 lines into one `@@` hunk, which is exactly the case where two intents (say a fix and a new function added 2 lines apart) can no longer be split. Zero-context diffs keep every edit as its own hunk.
- **`git apply --unidiff-zero`** is required to consume zero-context patches. Without it `git apply` rejects them as ambiguous because line-count math no longer works.
- **`--src-prefix=a/ --dst-prefix=b/`** overrides any local `diff.mnemonicPrefix` config so the patch always uses the `a/`/`b/` prefixes that `git apply` defaults to. Without this, a user who set `diff.mnemonicPrefix = true` will see `i/` and `w/` prefixes that `git apply` won't strip.

## Regenerate the diff every iteration

Always rebuild `cur.patch` from a fresh `git diff` — never reuse it across commits. Hunk numbering reflects the current HEAD: previously committed hunks disappear and remaining hunks are renumbered automatically. A reused patch will reference stale hunk indices and apply the wrong hunks (or nothing at all).

## Anti-pattern: don't rewrite the file in place

A tempting shortcut is to `git stash`, rewrite `<file>` to a partial state, commit, then `git stash pop` to restore the rest. This works for one or two commits but conflicts as soon as later commits overlap the same lines, and it loses the property that each commit is a real subset of the original diff. Stick to `git apply --cached` against fresh diffs.

## Mixed commit (some whole files + some partial hunks)

Run the partial-apply step first, then `git add` the whole-file additions, then commit once. Order matters because `git add <file>` would otherwise stage the *entire* file and overwrite the just-applied partial index entry.

## Special cases

- **Binary files.** No hunk split possible — commit whole or skip. `build_partial_patch.py` will reject `"hunks": [1, 3]` on a binary diff; use `"hunks": "all"`.
- **Pure rename without content changes.** Treat as `"hunks": "all"` — there are no hunks to filter.
- **Untracked files needing a partial split.** Run `git add -N <file>` first so the file appears in `git diff` and can be hunk-split. Skip this for files you plan to add whole.

## Helper script reference

`scripts/build_partial_patch.py` filters a unified diff to a selected subset of hunks. Run it via `python3` — it has no third-party dependencies. Read the file directly for the full JSON selection schema (the module docstring is the `--help`).
