# ADR-0016: Materialize mutable Claude configuration

- Status: Accepted
- Date: 2026-08-28
- Related: ADR-0008 and ADR-0014

In the context of deploying generated Claude Code configuration through Home Manager while Claude
Code may update its own settings, facing immutable Nix store symlinks that providers cannot safely
rewrite, we decided to materialize `CLAUDE.md`, `settings.json`, `hooks/`, and `skills/` as regular
user-owned files after Home Manager link cleanup, keep only the immutable statusline as a managed
symlink, recursively merge existing settings with generated settings taking precedence on the same
keys, and replace harness-owned hook and skill trees, and against recursive Home Manager links or
fully unmanaged settings, to preserve provider-owned state without losing declarative guardrails,
accepting an activation-time copy step and leaving the existing Codex synchronization model
unchanged.
