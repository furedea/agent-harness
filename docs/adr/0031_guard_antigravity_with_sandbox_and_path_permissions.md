---
status: accepted
date: 2026-11-20
---

# Guard Antigravity with the sandbox and path permissions

## Context and Problem Statement

Adding the Antigravity CLI (`agy`) provider ([#256](https://github.com/furedea/agent-harness/issues/256))
means its hook registration and `settings.json` are files the agent's own tools can write, and its
`permissions` grammar has six rule kinds, no `edit_file(...)`, and no globs (a glob in a path rule
makes the sandbox fail to start). How should the harness keep hooks, settings, and credentials out of
the agent's reach?

## Considered Options

* Hooks only.
* The standard `--sandbox` for commands plus generated `permissions` rules for file tools, with hooks
  kept for what rules cannot express.

## Decision Outcome

Chosen option: "sandbox plus generated permissions", because hooks alone cannot stop the agent from
rewriting the hook registration that carries them.

- Only concrete files and directories are emitted. Protected paths are `write_file(...)` denials; the
  whole `~/.gemini/config` directory and `settings.json` are denied.
- A secret path rule converts only when it names one location: `//x` becomes `/x`, `~` and `~/x` stay
  for the install or sync to expand, and a trailing `/**` names the directory. Any other glob and any
  project-relative pattern stays with the `PreToolUse` hook.
- `agy` reads absolute paths, so a sync, and an install whose prefix is the home directory, expand the
  leading `~` of path rules. A Nix build leaves them for the activation sync.
- The sandbox needs `read_file(/nix)` and `read_file(/run)` for `git` and `nix`. Narrower `/run` entries
  were not verified to be enough, so the grant stays broad and the well-known secret mounts under it
  are denied. Whether a deny wins over an allow is unverified.
- The harness owns only `command(`, `read_file(` and `write_file(` entries and pins `agentMode` to `""`
  (Default; Auto mode must not run) and `enableTerminalSandbox` to `true`. The rest of the user's
  settings is kept. `SandboxBypassDisabled` is not a settings key and is not managed.
- `~/.gemini/antigravity-cli` as a whole stays writable because `agy` keeps its own conversation and
  scratch files there; only `settings.json` is denied.
- With `github.agentToken`, the normal `gh` token and the App private key are denied with
  `read_file(...)`. `agy` settings have no key for the environment of its commands, so `GH_CONFIG_DIR`
  is not set by the harness and must come from the launch environment.

### Consequences

* Good, because hook registration, settings, and credentials are protected by the kernel sandbox and
  the rules instead of by the hooks they configure.
* Bad, because glob secret patterns are enforced by hooks only, and the `PreToolUse` matcher scope,
  deny/ask/allow precedence, `~/` interpretation, and `GH_CONFIG_DIR` delivery are unverified in an
  authenticated session.
