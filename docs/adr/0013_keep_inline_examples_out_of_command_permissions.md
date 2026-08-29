# ADR-0013: Keep inline examples out of command permissions

- Status: Accepted
- Date: 2026-08-29

In the context of a provider-neutral command-permissions source, facing Codex's optional inline
`match` and `not_match` tests without equivalent fields in other supported agents, we decided for
rules containing a decision, token prefix, and required justification and against requiring inline
command examples, to keep the shared public schema minimal and avoid implying that examples limit
runtime matching, accepting that generated Codex rules do not self-test every prefix when loaded
and repository tests remain responsible for generation behavior.
