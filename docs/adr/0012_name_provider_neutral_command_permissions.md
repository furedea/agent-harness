# ADR-0012: Name provider-neutral command permissions

- Status: Accepted
- Date: 2026-08-25
- Supersedes: ADR-0007

In the context of generating Claude Code Bash permissions and Codex execpolicy rules from one
provider-neutral file while hook guards also consume precise regular-expression rules, facing
ambiguous terms such as command policy and command prefix policy, we decided to name the shared
source `commandPermissions`, store it as `agents/command_permissions.json`, and model each rule with
an explicit `allow`, `ask`, or `deny` decision plus a token `prefix`, and against provider-specific
names or a generic pattern field, to make its intent and translation boundary visible at the public
API, accepting that Codex renders `ask` as `prompt`, `deny` as `forbidden`, and the separate hook
rule files continue to use regular-expression patterns.
