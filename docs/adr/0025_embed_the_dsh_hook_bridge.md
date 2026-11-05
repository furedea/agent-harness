# ADR-0025: Embed the dsh hook bridge

- Status: Accepted
- Date: 2026-11-05

In the context of adding DeepSeek Harness hooks ([#187](https://github.com/furedea/agent-harness/issues/187)),
facing a release-candidate plugin protocol and a manifest that must match its bridge,
we decided to embed a small bridge bundle in the binary, following pi and Hermes,
and against depending on `@deepseek-ai/dsh-hooks-claude-code`, to ship and verify the
adapter contract together, accepting responsibility for tracking dsh protocol changes.

The profile sync owns only its bundle dependency and ordered registration, preserving
user entries. Command permissions remain outside this decision. Compatibility of a
`link:` dependency into the Nix store with pnpm plugin operations remains to be
verified; copying the bundle under `DSH_HOME` is the fallback. Interactive `ask`
is deferred until its interaction with existing blocking hooks is specified.

Profile registration requires an existing dsh-initialized profile; activation skips
missing profiles with a warning so dsh can still apply its complete template.
Home Manager only registers the bundle and does not run a plugin installation.
The bundle becomes active after `dsh plugin --profile NAME install`. Its `link:`
target changes with each Home Manager generation, so rerun that command after
activation to reconcile the installed plugins with the current generation.

The bridge follows pi's failure policy: a missing adapter does nothing, and only
exit code 2 or an explicit block decision denies execution. Other adapter failures
are logged to stderr and allow execution, accepting reduced enforcement during
adapter outages to avoid denying every tool call.

Context messages use UUID strings and a producer-specific `agent-harness` source,
matching dsh's message identity and extensible source contracts without a runtime
npm dependency. The session working directory takes precedence over the process
working directory. Protection uses the configured dsh profile name.
