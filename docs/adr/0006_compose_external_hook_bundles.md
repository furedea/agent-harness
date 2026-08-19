# ADR-0006: Compose external hook bundles

- Status: Accepted
- Date: 2026-08-20
- Supersedes: ADR-0003

In the context of agent-harness owning shared Claude and Codex configuration while tools such as
Herdr and Moshi own their hook protocols, facing stale vendored artifacts and an increasing number
of tool-specific generators, we decided for caller-supplied versioned hook bundles captured through
a generic isolated installer runner and structurally composed by agent-harness, and against direct
installer writes or more tool-specific integrations, to keep tool release selection at the
composition root while retaining one validated configuration writer, accepting that callers must
pin installers, declare runtime command replacements, and update their bundles when upstream output
changes.
