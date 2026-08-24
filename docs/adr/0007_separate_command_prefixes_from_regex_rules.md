# ADR-0007: Separate command prefixes from precise regex rules

- Status: Accepted
- Date: 2026-08-24

In the context of sharing command policy across Claude Code and Codex while individual repositories
need narrowly scoped local command forms, facing provider permission formats based on token prefixes
and hook checks that require precise regular expressions, we decided to keep
`agents/command_policy.json` as the sole shared allow and forbidden prefix policy, store precise
global POSIX extended regular expressions in dedicated allowed and forbidden rule files, and permit
optional project rules under `<git-root>/.agents/hooks/rules/`, and against embedding policy arrays
in hook scripts or duplicating prefixes in provider-specific configuration, to keep generated
permissions and runtime checks aligned while allowing repository-local refinement, accepting that
project allow rules can only refine a shared allow prefix, invalid rule files fail closed, and every
forbidden match takes precedence.
