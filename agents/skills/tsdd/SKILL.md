---
name: tsdd
description: >
    Test-Spec Driven Development (TSDD): tests are the durable specification, TDD is the operating discipline, prose carries only Why, and `CLAUDE.md` stays a navigation map. Load before production code, tests, ADRs, architectural decisions, `CLAUDE.md` edits, or any durable prose artifact that would duplicate tests or code. Also load when the user asks to write tests or define test behavior before implementation, including "test 書いて", "test 書いて", "テスト書いて", "pytest", "テスト環境", "TSDD", "Test-Spec Driven Development", "TDD で実装", "ADR を書いて", "decision record", or "executable specification".
---

# Test-Spec Driven Development (TSDD)

## Scope

This skill governs the **methodology layer** of day-to-day coding work — how requirements, design decisions, and implementation are recorded and evolved, and in what order work proceeds. It is deliberately language-agnostic.

TSDD means the test suite is the durable specification. TDD is the operating discipline used to grow that specification one executable example at a time.

Out of scope and delegated elsewhere:

- Language-specific conventions (mock libraries, fixture patterns, lint rules, assertion style, package manager commands, module layout) → the relevant `*-style` skill and `rules/coding_guideline.md`.
- Project and CI bootstrap (flake, direnv, language init, CI workflow scaffolding) → the relevant `*-init` skill.
- ADR selection, repository inspection, layout, status, supersession, templates, and workflow → the `adr` skill.

If a rule below starts feeling like a language-specific implementation detail, it belongs in one of those skills, not here.

## Core Principle

**The spec is the test suite. Prose documents carry only Why.**

Natural-language specification documents become a second source of truth that drifts from the code, violates DRY, and doubles the cost of every requirement change. Under this methodology, requirements live inside executable tests, implementation is self-documenting via code and types, and the only human-written prose captures _why_ a decision was made — which code cannot express anyway, so no duplication occurs.

Tests are the durable encoding of accepted requirements, not their origin. A test name or assertion describes a requirement; it does not turn an arbitrary detail into one.

### Why this matters especially for AI coding agents

An AI agent's bottleneck is not typing speed. It is distinguishing "my output is correct" from "my output looks plausible." Executable specs give the agent an objective pass/fail signal that natural-language specs cannot provide. Tests fail when the agent hallucinates an API, types reject invalid states at compile time, and ADRs keep the agent from unknowingly reversing a past decision. Remove any one of these layers and the agent's autonomy degrades to plausibility checking.

## Information Placement Matrix

Every artifact in the codebase has exactly one of four homes. If the same information appears in two homes, delete it from the wrong one — do not "sync" them.

| Layer                            | Lives in                                                                                | Role                                                                                                |
| -------------------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| **What** (accepted requirements) | Tests                                                                                   | Executable specification. Cannot drift from code because it is executed against code.               |
| **How** (implementation)         | Code + types                                                                            | Self-documenting via naming, structure, Value Objects, and type constraints.                        |
| **Why** (decisions)              | ADR under `docs/adr/` for broad / architectural Why; inline code comments for local Why | Both carry rationale that code cannot express. They differ in scope (see "ADR vs inline comments"). |
| **Navigation**                   | `CLAUDE.md`                                                                             | Entry map for the AI agent. Points to the other layers; never copies them.                          |

**Anti-patterns**:

- A durable prose artifact that restates what the tests or code already say. Delete it; let the tests be the spec.
- A living document that is expected to be kept in sync with behavior or implementation. It will not be. Write tests and code instead.

**Exception**: in heavily regulated domains (medical device, safety-critical, finance with audit obligation) a natural-language spec may be legally required. In that case keep it, but treat the tests as the _authoritative_ spec and the document as a derivative that the tests cross-check.

## Transient Natural-Language Planning

Natural language is allowed as temporary working memory. It is often useful before the next executable example is clear, especially when decomposing a vague request, listing candidate behaviors, sketching edge cases, or comparing possible cuts through the problem.

The constraint is lifetime and authority: planning prose must never become an authoritative artifact in the repository. It may live in the chat, a scratchpad, or a temporary TODO while the work is being shaped, but it must be resolved before the task is finished.

Before finishing, every planning note must be converted into exactly one appropriate durable home:

- An accepted requirement or required observable behavior → an executable test with a full-sentence name.
- A domain invariant → a type, Value Object, parser, or boundary validation.
- An implementation detail → code whose names and structure make the detail clear.
- A broad rationale or rejected alternative → an ADR.
- A local rationale for a non-obvious line or block → an inline code comment.
- Navigation for future agents → a pointer in `CLAUDE.md`.
- Anything else → delete it.

Do not record scratchpads or TODO lists as durable project artifacts to preserve planning prose. If the prose still feels necessary after implementation, it is a signal that it has not yet been moved to the correct layer.

## TDD Operating Discipline

When current behavior has a real gap from an independently required observable contract, follow Kent Beck's Red → Green → Refactor one test at a time. Once Red is applicable, the rhythm is non-negotiable because breaking it is the single most common way AI agents regress into "write a pile of code, then bolt on tests."

### Red eligibility

Red is evidence of a real gap between current behavior and an independently required observable contract; it is not a deliverable to manufacture. Before writing a failing test, identify the contract and a requirement source outside the test or planned implementation. Valid sources include an explicit user requirement, a public interface or external standard, a reproduced defect, or behavior intentionally preserved for compatibility. A test name, planned implementation, current wording or internal structure, coverage target, or desire to demonstrate TDD does not create a requirement.

Use Red → Green → Refactor only when such a gap exists. Reuse an existing failing test when it already demonstrates the gap; never add or tighten an assertion solely to obtain Red. If no contract gap exists, establish a Green baseline and use the relevant verification gates. A passing characterization test is allowed when needed to protect required behavior during a behavior-preserving change.

### Per-cycle rules

These rules apply when Red is eligible:

1. **Establish one failing test for one contract gap.** Use an existing failing test when it already proves the gap; otherwise write one. Not three. Not ten. One.
2. **Run the verification gate and confirm the test fails for the expected reason.** Prefer the full suite. If the repo is large or already has unrelated failures, first establish the baseline, then run the narrowest suite that proves the executable spec is Red without hiding known failures.
3. **Write the minimum code to pass.** Obvious implementation if the path is clear; "fake it" (hardcode the answer) or triangulate with a second example if it is not.
4. **Run the verification gate and confirm Green.** Prefer the full suite; otherwise run the narrow suite plus the project's agreed quality gates, and explicitly report any pre-existing failures that remain.
5. **Refactor with the suite green.** Remove duplication, rename for clarity, extract functions / types. Re-run the suite after each small change.
6. **Record the change as one coherent VCS unit.** Commit each Red → Green → Refactor cycle so it remains reviewable as one logical unit.

### Test quality rules (methodology-level)

Only rules that are part of the development methodology live here. Mocking libraries, fixture patterns, assertion styles, coverage tooling — those are language-specific and belong in the relevant `*-style` skill.

- **Test names describe requirements; they do not create them.** The name is a human-readable statement of the independently required behavior being verified. `test_registering_with_empty_password_raises_validation_error` is a spec line; `test_1`, `test_user_ok`, `test_happy_path` are not. Treat the name as the specification channel, but never as the requirement source.
- **Expected outcome first.** Decide the observable outcome before arrange and act. When the language and framework make it natural, write the assertion first; otherwise keep the test shaped around the expected behavior, not around convenient setup.
- **Avoid overspecification.** Every assertion must protect an independently required observable contract. Omit wording, formatting, ordering, internal structure, and other details that can change without violating a requirement. Do not assert that Markdown contains prescribed prose as a proxy for underlying behavior. A behaviorally equivalent implementation or harmless rewording should pass; if it would not, relax or delete the assertion unless the exact representation is explicitly required by the contract.
- **One concept per test.** If the test name needs the word "and", split the test.
- **Do not re-test what the type system already guarantees.** If a parameter is `NonEmptyString`, do not write `test_empty_string_rejected` on every consumer — the constructor already enforces it. Test the invariant once, at the boundary where it is created.

### AI-specific failure modes and their guards

| Failure mode                                                                   | Guard                                                                                                                           |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Agent writes implementation first for a real contract gap, then bolts on tests | Require Red before implementation when Red is eligible; otherwise require a Green baseline                                      |
| Agent writes many tests all Red and then batch-implements                      | When Red is eligible, allow at most one Red test at a time; reintroduce additional tests one cycle at a time                    |
| Agent writes generic test names (`test_success`, `test_case_1`)                | Require full-sentence names describing independently required behavior; reject PRs otherwise                                    |
| Agent skips the Refactor step because tests are green                          | Treat Refactor as the mandatory final phase of an eligible cycle; point `CLAUDE.md` to TSDD instead of duplicating the rule     |
| Agent writes a test for behavior the type system already guarantees            | Review test diffs against type signatures; delete redundant coverage                                                            |
| Agent writes durable prose "to plan the feature"                               | Plan transiently, then resolve each note through the Information Placement Matrix; do not assume every plan item becomes a test |

## ADR (Architecture Decision Record) Operation

ADRs are the only long-form human-prose documents in this methodology. They carry broad Why and rejected alternatives that code and tests cannot express; inline code comments carry local Why.

Load the `adr` skill when a change introduces, reverses, or reviews an architectural decision or durable constraint. That skill owns the decision test, repository inspection, layout, status, supersession, templates, and workflow. Record an ADR in the same change as the code or tests it justifies.

## Type-Driven Design

Types carry compile-time invariants and constraints. They are not a second source of truth for requirements; they make invalid states unrepresentable so tests and runtime checks do not have to repeat those constraints.

### Patterns (naming is language-dependent — see the relevant `*-style` skill for idiomatic forms)

- **Value Objects** wrap primitives with domain constraints. `UserId`, `Email`, `PositiveInt`, `NonEmptyString` instead of raw `str` / `int`. Construct through a validating constructor; keep fields immutable.
- **Branded / newtype types** to prevent mixing structurally-identical but semantically-distinct values (e.g. `UserId` and `OrderId` are both `string` but should not be interchangeable).
- **Result / Either types** for operations that can fail. Forces the caller to handle both arms. Avoid throwing for expected failures.
- **Exhaustive sum types** for state machines. The compiler catches unhandled cases when a new variant is added.
- **Parse, don't validate.** Validation at the boundary turns raw input into a constrained type; once inside, the type guarantees correctness and downstream code stops re-checking.

### Interaction with tests

A test that only verifies what the type already guarantees is noise. Examples:

- Type is `NonEmptyString` → the constructor test covers emptiness; consumers do not need `test_empty_string_rejected`.
- Enum with exhaustive match → no need for `test_unknown_variant_raises`; the compiler forbids it.
- Return type is `Result<T, E>` → no need for `test_does_not_throw` on the happy path.

Tests should cover required behavior the type system cannot express: business rules, ordering, side effects, interaction with external systems.

## `CLAUDE.md` as the Agent's Entry Map

`CLAUDE.md` is the only document guaranteed to load into every task's context. Keep it a map, not a textbook. It should be read in under a minute.

### What belongs

- One-paragraph project summary (what, for whom, why it exists).
- One-line-per-top-level-directory layout intent.
- Coding conventions — or, preferably, pointers to the relevant `*-style` skill.
- Workflow rules phrased as invariants or pointers: "follow the `tsdd` skill's Red eligibility and applicable cycle", "ADR required for architectural decisions", "keep each verified change as one reviewable VCS unit".
- Explicit prohibitions: "do not push without user instruction", "do not create durable prose that duplicates tests or code".
- A pointer to `docs/adr/` as a resource to consult **on demand** when a decision-relevant topic arises. Do **not** auto-load the entire ADR directory into every task — it will bloat context as ADRs accumulate.

### What does not belong

- The actual list of requirements (those live in tests).
- Implementation details (those live in code).
- Decision rationales (those live in ADRs).
- Anything that needs to be edited every time a feature ships.

If `CLAUDE.md` grows past roughly 200 lines it has started duplicating another layer. Audit and extract.

## Enforcement Mechanisms

A methodology that relies on the agent remembering to follow it will drift. Build guardrails that make the right path the default:

- **TDD-enforcing workflow / wrapper command** that requires Red before implementation for a real contract gap, or a Green baseline when no gap exists, and never forces Red solely to advance.
- **CI gates**: tests pass; type checker clean.
- **PR template** with checkboxes: "Real contract gap?", "Red or Green baseline confirmed?", "ADR added or superseded?", "No durable prose duplicated tests or code?".
- **`CLAUDE.md`** points to `docs/adr/` as a consulting resource (read on demand), not a bulk-load target.

## Agent self-check before finishing a task

1. Was there a real gap between current behavior and an independently required observable contract? If yes, did a relevant test fail for the expected reason before the change and pass afterward? If no, did I establish a Green baseline without manufacturing Red?
2. Would a behaviorally equivalent implementation or harmless rewording pass each added or changed test? If not, is the exact representation explicitly required by the contract? If no → relax or delete the assertion.
3. Does each test name describe an independently required behavior without acting as its requirement source? If no → rename or delete the test.
4. Did I introduce an architectural choice not already covered by an ADR? If yes → load the `adr` skill.
5. Did I create durable prose that duplicates tests or code? If yes → delete it; move content into tests, ADR, or `CLAUDE.md` as appropriate.
6. Did I leave transient planning prose in the repository? If yes → convert it to the correct durable home or delete it.
7. Did I write a test for behavior the type system already guarantees? If yes → delete it.
8. Is `CLAUDE.md` still a map, or has it started duplicating another layer? If duplicating → prune.
9. Did I skip the Refactor step when Red was eligible because everything was green? If yes → return to it.

## Interaction with other skills

- **`*-style` (`python-style`, `bash-style`, `gha-style`, ...)** — language-specific conventions. This skill sits on top of them; both are usually loaded together when implementing.
- **`*-init` (`nix-dev-init`, `github-ci-init`)** — project and CI bootstrap. Runs _before_ this skill applies; this skill governs the code written _inside_ the bootstrapped project.

## Summary

Tests are the durable specification of accepted requirements, not their origin. Code and types are the design. ADRs carry broad reasoning. Inline comments carry local reasoning. `CLAUDE.md` is the map. Everything else is duplication waiting to rot — resist the instinct to write it.
