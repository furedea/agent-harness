//! Judgement backend interface for rules that only a model can decide (agent-config ADR-0020).
//!
//! A backend scores one unit against one rule. Hook wiring, caching, and thresholds are applied by
//! the callers (stage 3F); the backend only returns the raw score, or says the unit was not judged.

#![expect(
    dead_code,
    reason = "the Stop hook that calls the backend lands in stage 3F"
)]

pub(crate) mod jev_lint;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub(crate) trait JudgementBackend {
    fn judge(&self, unit: &JudgedUnit, rule: &JudgementRule) -> Result<Judgement>;
}

/// A file that a rule judges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JudgedUnit {
    pub(crate) path: PathBuf,
}

/// A model-judged rule: the id and version declared in the backend's own rule file, and that file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JudgementRule {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) file: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Judgement {
    Scored(RawScore),
    Unjudged(UnjudgedReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnjudgedReason {
    MissingApiKey,
}

/// The model's agreement that the unit violates the rule, in `0.0..=1.0`, before any cutoff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RawScore(f64);

impl RawScore {
    pub(crate) fn parse(value: f64) -> Result<Self> {
        (0.0..=1.0)
            .contains(&value)
            .then_some(Self(value))
            .ok_or_else(|| anyhow!("judgement score must be within 0.0..=1.0, got {value}"))
    }

    pub(crate) fn value(self) -> f64 {
        self.0
    }
}
