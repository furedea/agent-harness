# ADR-0002: Conditionally render Herdr integration

- Status: Superseded by ADR-0003
- Date: 2026-07-10

In the context of agent-harness owning generated Claude and Codex configuration while Herdr provides upstream session-reporting hooks, facing conflicting writers and duplicated ownership of Herdr protocol behavior, we decided to vendor Herdr-generated scripts without modification while conditionally composing their hook registration in agent-harness, and against running both installers or maintaining adapted script implementations, to keep agent-harness as the sole configuration writer while preserving Herdr's implementation, accepting an explicit Home Manager enablement setting and manual vendor refreshes when Herdr changes its integration artifacts.
