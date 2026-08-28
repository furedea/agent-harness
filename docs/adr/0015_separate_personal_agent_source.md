# ADR-0015: Separate personal agent source

- Status: Accepted
- Date: 2026-08-28
- Related: ADR-0014

In the context of publishing a reusable renderer while frequently changing one user's hooks,
skills, instructions, and provider settings, facing release work in this repository for every
personal policy update and a public default that encoded private opinions, we decided for keeping
only the neutral `minimal` source in agent-harness and owning complete personal source trees in the
caller's configuration repository, and against retaining an author-named built-in profile or
requiring a separate profile repository, to separate product releases from personal configuration
changes while preserving direct CLI and Home Manager composition, accepting that consumers must
pass their source explicitly and test source-specific behavior in its owning repository.
