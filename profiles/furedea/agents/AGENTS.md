# General

- Think in English, but generate responses in Japanese
- Use "，" and "．" in writing in Japanese, instead of "。" and "、"
- Directory names: Use hyphens (-) as separators (ex: claude-scripts)
- File names: Use underscores (\_) as separators (ex: lint_format.sh)
- Write documentation, code comments, and commit messages in English for public repositories
- When writing commit messages, follow Conventional Commits rules
- Implement based on Test-Spec Driven Development (TSDD)

# Coding Guidelines

Existing project style takes precedence over these rules.

- Keep files focused: prefer 80-120 columns, roughly 200-500 lines, high-level code before lower-level details, and related concepts close together.
- Separate object creation/configuration from execution logic.
- Keep classes and modules single-purpose, cohesive, loosely coupled, and minimally public.
- Name classes by responsibility; order methods public-to-private; use DTOs at component boundaries.
- Keep domain-specific enums and exception classes near their owning class; use one exception class per domain failure concept unless it is shared across modules.
- Keep functions small and single-purpose: prefer 0-3 arguments, one abstraction level, 2-4 lines when practical, short variable lifetimes, guard clauses, and at most one indentation level; split only when the extracted name clarifies intent.
- Split duplicated logic, control structures, mixed responsibilities, and command/query behavior into named functions or objects.
- Name one concept with one word; name command functions for their side effects and query functions for the value they return.
- Let name length match scope size; include units, trust/safety attributes, and boolean prefixes where they clarify meaning.
- Minimize comments and docstrings; use them for public APIs, TODOs, non-obvious constraints, and intent that code cannot express.
- Prefer DRY, YAGNI, and Law of Demeter.
- Apply SOLID pragmatically; introduce interfaces, polymorphism, or dependency inversion only at meaningful boundaries.
- Separate policy from details; delay database, framework, and external-service decisions behind abstractions when doing so reduces coupling.
- Prefer Value Objects over raw primitives for values with validation, invariants, or domain behavior
- Prefer Collection Objects for domain collections with invariants; do not expose mutable raw collections
- Use Entities only when stable identity matters across state changes
- Use classification objects/enums when categories or state transitions have domain rules
