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
