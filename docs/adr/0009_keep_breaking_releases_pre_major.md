# ADR-0009: Keep breaking releases pre-major

- Status: Accepted
- Date: 2026-08-24

In the context of agent-harness remaining below 1.0 while its public CLI and Home Manager
interfaces are still evolving, facing release-please's default promotion of any breaking commit to
1.0.0, we decided for minor version bumps on breaking changes before 1.0 and against treating the
first breaking change as a declaration of API stability, to preserve deliberate control over the
1.0 milestone while continuing to mark breaking changes explicitly, accepting that pre-1.0 minor
releases may require migration.
