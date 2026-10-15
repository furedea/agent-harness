# ADR-0020: Keep git push outside the Claude sandbox with the agent token

- Status: Accepted
- Date: 2026-10-15
- Supersedes: ADR-0019

In the context of the opt-in `programs.agent-harness.github.agentToken` option, which points
`GH_CONFIG_DIR` at an agent-scoped fine-grained token readable inside the Claude Code sandbox,
allows `github.com` and `api.github.com`, enables `sandbox.enableWeakerNetworkIsolation`, and
rejects a configuration whose Read denials hide the token directory, facing commit signing that
cannot run inside the sandbox because the macOS ssh-agent socket path changes per session and
`allowUnixSockets` has no wildcard, so `git commit *` stays excluded and a chained
`git commit -m x && git push` with only `git push *` moved inside matches neither side and runs
sandboxed where signing fails, we decided for removing only `gh *` from `sandbox.excludedCommands`,
even when the source settings list it, and keeping `git push *` and every other exclusion as the
source lists them, and against moving `git push *` inside as ADR-0019 did or running gh outside the
sandbox, to let add, commit, push, and status chain outside the sandbox while an excluded push still
authenticates with the agent token through the inherited `GH_CONFIG_DIR`, and to keep gh where the
sandbox denies reads of local secrets because gh outside it can upload any readable file through
`gh gist create`, `gh release upload`, or `gh api --input` whereas git push can only send repository
commits, accepting that the source settings must exclude `git add *` and `git status *` alongside
`git commit *` and `git push *` for such chains, that the token's fine-grained permissions remain the
safety boundary for sandboxed code, that this composition remains the one override of the source
settings that ADR-0018 otherwise treats as authoritative, and that remotes must use HTTPS and only
Claude Code is covered so far.
