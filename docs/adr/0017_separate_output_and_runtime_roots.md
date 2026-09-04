# ADR-0017: Separate output and runtime roots

- Status: Accepted
- Date: 2026-09-04

In the context of rendering the same source into both user-level and project-local provider
configuration, facing hook commands and protection rules that previously assumed `$HOME` even when
files were written elsewhere with `--prefix`, we decided for an explicit absolute runtime root that
is independent from the output prefix, and against inferring scope from the prefix or rewriting
source files in place, to make project-local installations self-contained while preserving the
existing home installation default, accepting one additional install option for non-home layouts.
