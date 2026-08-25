# ADR-0010: Use pre-major compatibility versions

- Status: Accepted
- Date: 2026-08-25
- Supersedes: ADR-0009

In the context of agent-harness remaining below 1.0 while its public CLI and Home Manager
interfaces are still evolving, facing minor releases that do not distinguish compatible features
from breaking changes, we decided for patch releases on backward-compatible changes, minor releases
on breaking changes, and an explicit promotion to 1.0, and against rewriting published versions or
using minor releases for every feature, to make pre-major compatibility boundaries visible while
preserving the existing release history, accepting that feature releases before 1.0 use patch
versions rather than conventional post-1.0 SemVer increments.
