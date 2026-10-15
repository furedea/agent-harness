# ADR-0019: Opt in to an agent GitHub token inside the Claude sandbox

- Status: Accepted
- Date: 2026-10-15

In the context of Claude Code running `gh` and `git push` outside the sandbox through
`sandbox.excludedCommands` because the user's GitHub token is unreadable inside it, facing chained
calls such as `git commit -m x && git push` that run sandboxed whenever one command is not excluded,
fail authentication, and end in a `dangerouslyDisableSandbox` approval prompt, we decided for an
opt-in Home Manager option `programs.agent-harness.github.agentToken` that points `GH_CONFIG_DIR` at
a separate directory holding an agent-scoped fine-grained token, allows `github.com` and
`api.github.com`, enables `sandbox.enableWeakerNetworkIsolation`, removes `gh *` and `git push *`
from `sandbox.excludedCommands` even when the source settings list them, and rejects a
configuration whose Read denials hide that directory, and against reading the user's own token
inside the sandbox, keeping the exclusions in the source settings, or issuing tokens automatically,
to let agents push and open pull requests without leaving the sandbox while the user's own token
stays denied, accepting that the token's fine-grained permissions rather than command rules are the
safety boundary because any sandboxed code can use it, that this one composition overrides the
source settings that ADR-0018 otherwise treats as authoritative, and that remotes must use HTTPS
and only Claude Code is covered so far.
