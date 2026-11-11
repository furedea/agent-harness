# ADR-0029: Run Pi file operations inside srt

- Status: Accepted
- Date: 2026-11-12

In the context of [Pi sandboxing](https://github.com/furedea/agent-harness/issues/221),
facing path aliases, hard links, filesystem case folding, and incomplete hook coverage,
we decided for executing file operations in a helper process inside the same srt boundary
as bash and against reproducing filesystem permission checks in hooks, to let the kernel
enforce generated read and write restrictions, accepting process startup overhead per
operation and excluding extension and MCP tools from sandboxed sessions.

Read, write, and edit use Pi's operations interface. Search tools execute entirely in the
helper because grep launches ripgrep outside that interface. Hooks remain an external
policy layer and retain their normal behavior on adapter failures. The helper is untrusted;
its implementation must never be relied on to enforce permissions.

The runtime requires recursive deny-glob support. nixpkgs sandbox-runtime 0.0.52 is suitable
only for probes because its glob denials do not protect descendants consistently.
