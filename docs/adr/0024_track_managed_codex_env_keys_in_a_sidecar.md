# ADR-0024: Track managed Codex env keys in a sidecar

- Status: Accepted
- Date: 2026-10-26

In the context of syncing the generated `shell_environment_policy.set` into the user's
`~/.codex/config.toml`, which Codex and the user also write to, facing the fact that a key absent
from the generated source cannot be told apart from one the user or Codex added, we decided for a
sidecar `.agent-harness-managed.json` next to `config.toml` that lists the keys last written, so a
sync removes only listed keys the source dropped, and against a hardcoded key allowlist, markers
inside `config.toml`, and never removing keys, to sync every generated key without deleting keys
Codex or the user own, accepting a new persistent file, and that a missing or unreadable sidecar
forgoes removals (except for the legacy `GH_CONFIG_DIR`) until the next sync rewrites it.
