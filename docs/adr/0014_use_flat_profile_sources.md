# ADR-0014: Use flat profile sources

- Status: Accepted
- Date: 2026-08-28
- Supersedes: ADR-0011
- Amends: ADR-0012

In the context of keeping a personal multi-provider configuration directly under a dotfiles
`agents/` directory, facing redundant `agents/agents/` nesting and runtime requirements hardcoded
to built-in profile names, we decided for flat profile roots with shared files at the root,
provider-specific `claude/` and `codex/` directories, and a versioned manifest that declares
runtime commands, and against a nested provider-neutral `agents/` directory or profile-enum-owned
requirements, to make external profile sources first-class and portable across Nix and standalone
installs, accepting a breaking source-layout migration before 1.0. The `commandPermissions` option
and `command_permissions.json` name remain unchanged; only the file's relative location changes.
