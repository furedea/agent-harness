# ADR-0018: Preserve source Claude permission rules

- Status: Accepted
- Date: 2026-09-26

In the context of generating Claude Code `permissions` from a source `claude/settings.json` and the
provider-neutral `command_permissions.json`, facing Claude-only rules such as
`Bash(dangerouslyDisableSandbox:true)` that are not command prefixes and silently vanished because
every source `Bash(...)` entry was treated as generator-owned, we decided for keeping every source
`allow`, `ask`, and `deny` entry, appending the generated entries, listing each rule once, and
keeping a rule declared in several lists only in its most restrictive list, and against adding
provider-specific rule syntax to `command_permissions.json` or stripping only prefix-shaped
`Bash(...)` entries, to make the source settings the authoritative home for Claude-only rules
without letting the merge widen a permission, accepting that a stale source `Bash(...)` rule is no
longer removed automatically when its prefix leaves `command_permissions.json`.
