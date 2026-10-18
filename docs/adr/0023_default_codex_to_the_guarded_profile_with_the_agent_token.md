# ADR-0023: Default Codex to the guarded profile with the agent token

- Status: Accepted
- Date: 2026-10-19

In the context of the opt-in agent GitHub token of ADR-0019, which Codex shell commands receive
through `shell_environment_policy.set.GH_CONFIG_DIR`, facing a Codex sandbox whose legacy
`sandbox_mode` cannot deny reading `~/.config/gh/hosts.yml` or the GitHub App private key, and a
`guarded` permissions profile that listed only harness files and therefore blocked ordinary
read-only commands, we decided for making `guarded` the default (`default_permissions = "guarded"`)
when the agent token is enabled and widening it to read `:root`, write `:workspace_roots`,
`:tmpdir`, `:slash_tmp`, and the absolute and `~/` paths in Claude's `sandbox.filesystem.allowWrite`,
enable network, and deny the normal gh token and the App key, and against a launch wrapper that adds
each repository's git common directory and worktree git directory as writable paths at startup, or
keeping the legacy `sandbox_mode`, to give Codex the same credential isolation as Claude without
another entry point that every launcher, including Herdr, must use, accepting that Codex keeps a
linked worktree's git directory read-only unless the profile names its exact path, so commits from
a worktree stay approval-gated in Codex while commits in a main checkout run inside the sandbox, and
that network access is not limited by domain.
