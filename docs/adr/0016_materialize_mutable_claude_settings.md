---
status: accepted
date: 2026-08-29
---

# ADR-0016: Materialize mutable Claude settings

In the context of deploying generated Claude Code settings through Home Manager while Claude Code
may update the same file, facing immutable Nix store symlinks and provider-owned state, we decided
for materializing `settings.json` as a writable regular file, replacing generated top-level values
and preserving existing top-level keys absent from the generated settings, and against an immutable
symlink, recursive JSON merging, or materializing all Claude assets, to preserve provider state
without weakening generated guardrails, accepting that a key removed from the generated settings is
not automatically removed from an existing file.
