# ADR-0011: Compose profiles with caller sources

- Status: Accepted
- Date: 2026-08-25

In the context of publishing agent-harness as a reusable Codex and Claude Code composition layer
while the repository also contains personal skills, hooks, and policies, facing a choice between
removing useful personal assets and making every Nix user maintain a separate profile repository,
we decided for a neutral `minimal` default profile, an opt-in `furedea` profile that preserves the
existing environment, and caller-supplied `AGENTS.md`, command permissions, skills, hook bundles,
and provider settings composed through the Home Manager module, and against a single opinionated
default or mandatory profile repositories, to make the reusable mechanism clear without discarding
a working reference configuration, accepting that profile selection and direct source attributes
are breaking changes and that callers own collisions among their injected sources.
