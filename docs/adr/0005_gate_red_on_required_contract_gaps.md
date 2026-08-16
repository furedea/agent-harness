# ADR-0005: Gate Red on required contract gaps

- Status: Accepted
- Date: 2026-08-16

In the context of AI agents following TSDD, facing brittle assertions manufactured to satisfy an
unconditional Red requirement, we decided for requiring Red only when current behavior has a real
gap from an independently required observable contract and against universal Red or an
overspecification warning alone, to preserve behavior-focused executable specifications without
weakening TDD for behavior changes and defects, accepting that behavior-preserving and
verification-only work begins from a Green baseline.
