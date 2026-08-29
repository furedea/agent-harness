# ADR-0016: Materialize mutable Claude settings

- Status: Accepted
- Date: 2026-08-28
- Related: ADR-0008 and ADR-0014

In the context of deploying generated Claude Code configuration through Home Manager while Claude
Code may update its own settings, facing immutable Nix store symlinks that providers cannot safely
rewrite, we decided to materialize only `settings.json` as a regular user-owned file after Home
Manager link cleanup, recursively merge existing settings with generated settings taking precedence
on the same keys, and keep `CLAUDE.md`, hooks, skills, and the statusline as managed symlinks, and
against either an immutable settings symlink or materializing the entire Claude harness, to preserve
provider-owned state without giving up declarative ownership of read-only harness assets, accepting
an activation-time settings synchronization step and leaving the existing Codex synchronization
model unchanged.
