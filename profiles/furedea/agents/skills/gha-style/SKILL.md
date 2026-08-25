---
name: gha-style
description: >
    GitHub Actions workflow coding conventions: security-critical patterns including permissions, action version pinning, script injection prevention, timeout, shell settings, and concurrency. Load whenever writing or reviewing .github/workflows/*.yml, creating composite actions (.github/actions/), or discussing CI/CD pipeline design. Trigger on: workflow yaml, github actions, CI/CD, .yml in .github/, actions/checkout, ubuntu-latest, workflow_dispatch, on: push, on: pull_request, jobs:, steps:, run:.
---

# GitHub Actions Coding Conventions

Claude already knows the basic structure of GitHub Actions (events, workflows, jobs, steps). Focus on the following patterns that are easy to overlook but important for security and reliability.

## 1. Permissions — Whitelist Approach

The default `GITHUB_TOKEN` has read/write access to the repo, which is broader than most jobs need. Declare `permissions: {}` at the workflow level (deny-all), then grant only what each job actually requires.

```yaml
permissions: {} # workflow level: deny-all

jobs:
    build:
        permissions:
            contents: read # checkout
            pull-requests: write # post comments
```

Common permission keys: `contents`, `issues`, `pull-requests`, `packages`, `id-token` (OIDC), `checks`, `statuses`.

## 2. Version Pinning

Tags like `@v4` can be moved or deleted by the action author — the same tag may point to different code over time. Pin to an exact version tag or commit SHA.

```yaml
# Avoid — tag can move
- uses: actions/checkout@v4

# Recommended — exact version tag
- uses: actions/checkout@v4.2.1

# High security — immutable SHA (add the tag as a comment for readability)
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.1
```

## 3. Script Injection Prevention

GitHub context values like PR titles, issue bodies, and branch names come from untrusted external sources. Interpolating them directly into `run:` allows an attacker to inject arbitrary shell commands. Always pass them through `env:` instead.

```yaml
# Dangerous — PR title injected directly into shell
- run: echo "${{ github.event.pull_request.title }}"

# Safe — value passed as environment variable (shell-escaped automatically)
- env:
      TITLE: ${{ github.event.pull_request.title }}
  run: echo "$TITLE"
```

Untrusted inputs include: `github.event.pull_request.title`, `github.event.issue.body`, `github.head_ref`, `github.event.*.name`, any user-controlled field.

## 4. timeout-minutes

The default job timeout is 6 hours. A hung step will consume runner minutes silently. Set an explicit timeout appropriate for the job — typically 5–15 minutes for CI.

```yaml
jobs:
    build:
        timeout-minutes: 10
```

## 5. Shell Settings

Explicitly specifying `shell: bash` causes GitHub Actions to run steps with `bash --noprofile --norc -eo pipefail`, which enables `pipefail` — without it, a failed command in a pipe chain (e.g. `cmd | grep`) can silently succeed.

Set it at the `defaults.run` level to apply to all steps in the workflow or job:

```yaml
defaults:
    run:
        shell: bash
```

For debugging, add `set -x` at the top of a `run:` block to print each command and its output to stderr before execution:

```yaml
- run: |
      set -x
      uv run pytest
```

## 6. Concurrency

When the same workflow can be triggered multiple times simultaneously for the same ref (e.g. rapid commits to a PR branch), earlier runs are typically wasted work. Use `concurrency` to cancel stale runs automatically.

```yaml
# At workflow top-level — cancel previous runs on the same branch/PR
concurrency:
    group: ${{ github.workflow }}-${{ github.ref }}
    cancel-in-progress: true # use false for deploy workflows that must run in order
```

This is most useful for `pull_request` and `push` triggers where users push multiple commits in quick succession. Each branch/PR gets its own group, so cancellation is scoped — PR #42 runs do not affect PR #43 runs.

## 7. Runner: Blacksmith

For GitHub Organizations, Blacksmith is a drop-in replacement for `ubuntu-latest` (faster CPU + local NVMe cache). Single line change:

```yaml
- runs-on: ubuntu-latest
+ runs-on: blacksmith-4vcpu-ubuntu-2204
```

Constraints: Organizations only, Ubuntu 22.04 only, 25 GB cache/repo. 3,000 free minutes/month to try.
