---
name: adr
description: >
    Architecture Decision Record (ADR) operation skill. Load when a task involves architectural decisions, decision records, ADRs, docs/adr/, library or framework selection, durable project constraints, rejected alternatives, superseding or deprecating previous decisions, or when TSDD says a broad Why belongs in an ADR. This skill turns decision-relevant changes into docs/adr/ entries instead of leaving ADR creation implicit.
---

# Architecture Decision Records

This skill governs the execution layer for ADRs: deciding whether an ADR is needed, reading the existing decision history, creating or superseding ADR files, and keeping decision prose out of tests and implementation.

Use this together with `tsdd` when the work involves both executable behavior and architectural rationale. `tsdd` owns the methodology; this skill owns the mechanics of recording the decision.

## Core Rule

When a change introduces or reverses a broad project decision, create or update an ADR under `docs/adr/` as part of the same task.

Do not leave ADR creation as an optional follow-up. If the rationale matters for future maintainers or future agents, record it before finishing the work.

## When To Load

Load this skill before making or reviewing changes involving:

- Architecture Decision Records, decision records, ADRs, or `docs/adr/`.
- Library, framework, database, protocol, hosting, or tooling selection where real alternatives exist.
- Architectural patterns such as server-rendered vs SPA, sessions vs tokens, monolith vs services, sync vs async, polling vs events, or generated vs handwritten artifacts.
- Durable constraints that are not obvious from code, such as "no runtime dependencies", "Nix owns this configuration", or "tests are the only requirements source".
- Reversing, superseding, deprecating, or intentionally rejecting a previous decision.

Do not load it for a local implementation detail that can be understood from the code in under a minute.

## Decision Test

Before writing an ADR, ask:

1. Would a future maintainer ask "why this approach instead of the obvious alternative?"
2. Does the Why span multiple files, components, workflows, or future changes?
3. Were meaningful alternatives considered or rejected?
4. Would an inline comment either be too narrow or need to be repeated in several places?

If any answer is yes, create an ADR. If all answers are no, prefer code, tests, or a local comment.

## Repository Inspection

Before creating or changing ADRs:

1. Check whether `docs/adr/` exists.
2. Read existing ADR titles and metadata before choosing a new decision.
3. Search existing ADRs for the topic, chosen option, and rejected alternatives.
4. If an existing ADR already covers the decision, update code/tests to follow it instead of writing a duplicate.
5. If the new decision reverses or materially changes an old one, write a new ADR and mark the old one as superseded.

Use `rg --files docs/adr` and targeted `rg` searches when available.

## File Layout

Store ADRs here:

```text
docs/
└── adr/
    ├── 0001-record-architecture-decisions.md
    ├── 0002-<decision-kebab-case>.md
    └── 0003-<decision-kebab-case>.md
```

Rules:

- Use one decision per file.
- Use sequential four-digit IDs.
- Never reuse an ID.
- Never delete old ADRs to hide history.
- Use kebab-case for the filename decision slug.
- Create `docs/adr/` if it does not exist.
- If the repository has no ADRs yet and the task adopts ADRs as a practice, create `0001-record-architecture-decisions.md` first.

## Status Rules

Allowed status values:

- `Proposed`
- `Accepted`
- `Superseded`
- `Deprecated`

Default to `Accepted` when the decision is implemented in the same change. Use `Proposed` only when the user asked for a proposal or no implementation change is being made.

Past ADRs are immutable except for metadata status changes. Do not rewrite the body of an accepted ADR to make it match a new decision.

When reversing a decision:

1. Create a new ADR with the next sequential ID.
2. Add `- Supersedes: ADR-NNNN` to the new ADR metadata.
3. Change the old ADR status to `Superseded`.
4. Keep both ADRs in the same commit or change set.

## Preferred Template

Use the Y-Statement form by default:

```markdown
# ADR-NNNN: <Decision title>

- Status: Accepted
- Date: YYYY-MM-DD

In the context of <use case or situation>, facing <concern or forcing function>, we decided for <chosen option> and against <alternatives considered>, to achieve <quality or benefit gained>, accepting <downside or cost incurred>.
```

For superseding ADRs:

```markdown
# ADR-NNNN: <Decision title>

- Status: Accepted
- Date: YYYY-MM-DD
- Supersedes: ADR-0003

In the context of <use case or situation>, facing <concern or forcing function>, we decided for <chosen option> and against <alternatives considered>, to achieve <quality or benefit gained>, accepting <downside or cost incurred>.
```

Use the longer template only when a Y-Statement would be too compressed:

```markdown
# ADR-NNNN: <Decision title>

- Status: Accepted
- Date: YYYY-MM-DD

## Context

## Decision

## Alternatives Considered

## Consequences
```

## Writing Rules

- Write ADRs in English for public repositories.
- Record Why, not What or How.
- Do not duplicate requirements that belong in tests.
- Do not duplicate implementation details that belong in code and types.
- Name rejected alternatives explicitly.
- State the accepted trade-off plainly.
- Keep the ADR short enough that future agents will actually read it.
- Link to related ADRs only when the relationship changes interpretation.

## Workflow

For decision-relevant implementation tasks:

1. Load `tsdd` and any language-specific skill required by the code.
2. Establish the executable behavior with tests when the change has behavior.
3. Inspect existing ADRs before choosing or recording the decision.
4. Implement the code change.
5. Create or supersede the ADR before finishing.
6. Run the relevant verification gate.
7. Report the ADR path and the verification result.

For documentation-only decision tasks:

1. Inspect existing ADRs.
2. Decide whether this is a new decision, a duplicate, or a supersession.
3. Create or update only the necessary ADR files.
4. Run the repository's markdown or generated-artifact checks when available.
5. Report the ADR path.

## Non-Goals

Do not create ADRs for:

- Ordinary refactors with no architectural choice.
- Language or framework defaults with no project-specific reason.
- Test cases, acceptance criteria, or behavior specs.
- Local code comments that explain one narrow line or block.
- Scratch planning or TODO lists.
