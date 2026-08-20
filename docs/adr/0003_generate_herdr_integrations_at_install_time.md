# ADR-0003: Generate Herdr integrations at install time

- Status: Superseded by ADR-0006
- Date: 2026-07-12
- Supersedes: ADR-0002

In the context of agent-harness owning shared Claude and Codex configuration while Herdr owns its session-reporting implementation, facing stale vendored scripts and conflicting configuration writers, we decided to run the installed Herdr CLI against a temporary home when integration is explicitly enabled, then structurally merge its generated hooks and Codex feature settings into agent-harness output, and against environment-based auto-detection or repository-owned copies of Herdr artifacts, to preserve upstream behavior without requiring Herdr for other users, accepting that enabled installations require an available Herdr binary and fail when its installer fails.
