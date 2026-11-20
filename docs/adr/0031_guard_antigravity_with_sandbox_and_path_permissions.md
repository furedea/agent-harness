---
status: accepted
date: 2026-11-20
---

# ADR-0031: Guard Antigravity with the sandbox and path permissions

In the context of adding the Antigravity CLI (`agy`) provider
([#256](https://github.com/furedea/agent-harness/issues/256)), facing a settings file
that is itself a tool-writable guardrail and a `permissions` grammar without globs,
we decided to combine the standard `--sandbox` for commands with generated
`permissions` rules for file tools, and against relying on hooks alone, to keep hook
registration and settings out of the agent's reach, accepting that glob secret patterns
are enforced by hooks only.

`permissions` takes six rule kinds; there is no `edit_file(...)`, so protected paths are
`write_file(...)` denials. A glob in `read_file(...)` or `write_file(...)` makes the sandbox
fail to start, so only concrete files and directories are emitted. A secret path rule
converts only when it names one location: `//x` becomes `/x`, `~/x` stays, a trailing
`/**` names the directory. Every other pattern remains with the `PreToolUse` hook.
`agy` reads absolute paths, so the sync expands the leading `~/` of path rules.

The sandbox needs `read_file(/nix)` and `read_file(/run)` for `git` and `nix`; narrower `/run`
entries were not verified to be enough, so the grant stays this broad and relies on the
generated denials beneath it. `~/.gemini/antigravity-cli` as a whole stays writable because
`agy` keeps its own conversation and scratch files there; only `settings.json` is denied. The
harness owns only `command(`, `read_file(` and `write_file(` entries, and pins
`agentMode` to `""` (Default; Auto mode must not run) and `enableTerminalSandbox` to
`true`. Everything else in the user's settings is kept. `SandboxBypassDisabled` is not
a settings key and is not managed.

With `github.agentToken`, the normal `gh` token and the App private key are denied with
`read_file(...)`. `agy` settings have no key for the environment of its commands, so
`GH_CONFIG_DIR` is not set by the harness and must come from the launch environment;
this is unverified end to end. The `PreToolUse` matcher scope, deny/ask/allow precedence,
and `~/` interpretation in an authenticated session are also unverified.
