# ADR-0008: Generate the runtime protection policy

- Status: Accepted
- Date: 2026-08-24

In the context of protecting installed harness files across Claude permissions, Claude sandboxing,
Codex filesystem guards, and explanatory runtime hooks, facing duplicated path lists that can drift
between the hard boundary and the hook, we decided for the Rust protection inventory as the sole
definition and a generated JSON policy consumed by the runtime hook, and against handwritten Bash
path patterns or separately maintained provider lists, to keep every enforcement layer aligned,
accepting that the installed hook depends on a valid generated policy and fails closed when that
policy is unavailable.
