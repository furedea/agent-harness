# ADR-0004: Inject external skills at install time

- Status: Accepted
- Date: 2026-08-09

In the context of agent-harness installing shared skills while upstream tools such as Herdr own
release-matched agent instructions, facing stale vendored copies and undesirable dependencies on
concrete tool packages, we decided for caller-supplied external skill directories composed through
the Home Manager module, and against vendoring upstream skills or adding tool-specific skill
integrations, to keep agent-harness dependent only on a generic skill-source boundary while
preserving upstream files verbatim, accepting that the composition root must build and provide each
external skill directory.
