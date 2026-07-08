# ADR-0001: Separate Codex conformance from required CI

- Status: Accepted
- Date: 2026-07-08

In the context of generating Codex execpolicy rules from repository-owned policy, facing frequent local Codex CLI updates and upstream behavior changes outside this repository, we decided for required tests that validate repository-owned generation behavior plus a separate push/pull-request Codex conformance workflow against the latest CLI, and against pinning Codex CLI in the default development shell or folding it into the repository-owned CI job, to keep the repository policy and tests as the stable source of truth, accepting that latest-Codex breakage can fail independently of the repository-owned checks.
