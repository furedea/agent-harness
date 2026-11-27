---
status: accepted
date: 2026-11-26
supersedes: [ADR-0020, ADR-0021, ADR-0022, ADR-0023, ADR-0030]
---

# Limit the harness to rendering and installing

## Context and Problem Statement

agent-harness turns one neutral source into each harness's native files and installs them. Over
time it also took on decisions that differ per user and products of their own
([#262](https://github.com/furedea/agent-harness/issues/262)): which installed files the agent may
not edit (the protection inventory of ADR-0008 and the protection part of ADR-0031), how the agent
authenticates to GitHub (the `github.agentToken` composition and the `github-app-token` subcommand
of ADR-0019 to ADR-0023), and an srt integration that replaces Pi's tools (ADR-0030). These embed
policy in a rendering tool, add credentials handling and a sandbox implementation to its trust
base, and override the source settings that ADR-0018 treats as authoritative. Where does each of
them belong?

## Considered Options

- Keep the three areas in agent-harness.
- Move each area into a separate package that agent-harness installs like a skill or hook bundle.
- Limit agent-harness to rendering the source and installing the result; the source (agent-config)
    owns policy, and the user's system configuration (dotfiles) owns credentials and sandbox
    launchers.

## Decision Outcome

Chosen option: "limit agent-harness to rendering and installing", because every one of these
decisions can be written as plain source settings that the harness already renders, so keeping
them in the harness, or in packages it ships, keeps per-user policy in a shared tool without adding
anything the source cannot express.

- agent-harness includes: reading the neutral source, rendering each provider's native files from
    it, merging them with the installed mutable configs it declares ownership of, and installing,
    verifying, and listing them.
- agent-harness excludes: deciding what is protected or allowed beyond what the source declares,
    holding or minting credentials, and implementing or launching sandboxes.
- Protection becomes a write allowlist in each harness's sandbox plus one deny per harness root for
    file tools that run outside it, written by the source. Token minting and refresh move to the
    user's system configuration, connected to the source only through `GH_CONFIG_DIR`. The Pi srt
    extension is removed without a replacement in this repository.
- The removals land as separate changes, each superseding or amending the ADRs it retires.
    Installing over an earlier release removes the files that release installed and this one no
    longer does, so a retired extension does not keep loading.

### Consequences

- Good, because the source settings become the only authority for permissions again, without the
    ADR-0018 exception, and the harness no longer touches App keys or tokens.
- Good, because a harness root deny covers files added later, where an inventory of installed
    files has to be regenerated to cover them.
- Bad, because each removal is a breaking change: consumers must add the equivalent source settings
    and launchers before upgrading, or the protection, token supply, or isolation silently stops.
- Bad, because Pi has no OS-level isolation from agent-harness after the srt extension is removed.
