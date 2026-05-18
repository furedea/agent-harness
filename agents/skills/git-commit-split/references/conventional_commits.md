# Conventional Commits — type, scope, subject, body

Read this in **Phase 2 (plan)** when you need to assign a type/scope/subject to a planned commit. The rules here are intentionally compact; the *why* is explained where it isn't obvious.

## Choose the type

Pick the type that best describes the *primary* effect of the change. If the commit genuinely combines a feature and its tests, use `feat:` — the tests are part of delivering the feature, not a separate concern.

| type | when to use |
| --- | --- |
| `feat` | new user-visible capability |
| `fix` | bug fix |
| `refactor` | restructure without behavior change |
| `perf` | performance-only change |
| `docs` | documentation only |
| `test` | test-only change (adding/fixing tests for *existing* code) |
| `build` | build system, packaging, deps (`package.json`, `pyproject.toml`, lockfiles) |
| `ci` | CI configuration only (`.github/workflows`, etc.) |
| `chore` | tooling/config that doesn't fit elsewhere |
| `style` | formatting, whitespace, semicolons — no logic change |
| `revert` | reverts a previous commit |

## Choose the scope

Conventional Commits scope is optional. The convention here:

1. If all files in the commit live under one identifiable module/area, use that area's name as scope (typically the directory basename — e.g., `src/auth/login.ts` → `auth`).
2. If the commit spans multiple top-level areas or sits at the repo root, **omit the scope**.
3. Scopes are lowercase, single-word, no slashes.

Scope is for the commit message only; it is intentionally **not** carried into the branch name in `pr-per-feature` mode (see `references/pr_per_feature_execute.md`). Branch names with parentheses or nested slashes confuse some tools.

## Write the subject

- Imperative mood ("add", "fix", "remove" — not "added", "adds", "fixed").
- Lowercase first letter, no trailing period.
- Aim for ≤50 characters; hard cap at 72.
- Describe the change, not the file ("add JWT refresh flow", not "update auth.ts").

## Optional body

Add a body only when the *why* isn't obvious from the subject. Wrap at ~72 columns. Skip it for trivial changes — empty bodies are better than filler.

## Examples

```
feat(auth): add JWT refresh-token rotation
fix(parser): handle empty input without panicking
refactor(db): extract query builder from repository
docs: clarify install steps for Apple Silicon
test(auth): cover refresh-token expiry edge case
build(deps): bump axios from 1.6.0 to 1.7.2
chore: ignore .DS_Store
revert: revert "feat(auth): add JWT refresh-token rotation"
```

## Language

Detect the language used in recent `git log --oneline -n 20` output. If recent history is non-English, match it; otherwise default to English (the Conventional Commits convention). The Conventional Commits *prefix* (`feat:`, `fix:`, …) stays ASCII regardless of subject language.
