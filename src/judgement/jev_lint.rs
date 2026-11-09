//! `mizchi/jev-lint` as the judgement backend.
//!
//! The binary comes from the Nix package (`packages.<system>.jev-lint`), never from `npx`. The API
//! key is read only by the child process; this module checks that a key variable is set and never
//! reads, logs, or stores its value.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::{JudgedUnit, Judgement, JudgementBackend, JudgementRule, RawScore, UnjudgedReason};

const API_KEY_VARIABLES: [&str; 2] = ["TYPESAFE_API_KEY", "TYPESAFEAI_API_KEY"];

pub(crate) trait CommandRunner {
    /// Runs `program` and returns its standard output whatever the exit status is, because jev-lint
    /// may exit non-zero when it reports findings.
    fn run(&self, program: &Path, args: &[OsString]) -> Result<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiKey {
    Present,
    Absent,
}

impl ApiKey {
    pub(crate) fn from_environment() -> Self {
        api_key_from(
            API_KEY_VARIABLES
                .iter()
                .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty())),
        )
    }
}

fn api_key_from(is_set: bool) -> ApiKey {
    if is_set {
        ApiKey::Present
    } else {
        ApiKey::Absent
    }
}

pub(crate) struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, program: &Path, args: &[OsString]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {}", program.display()))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

pub(crate) struct JevLintBackend<R> {
    binary: PathBuf,
    api_key: ApiKey,
    runner: R,
}

impl<R: CommandRunner> JevLintBackend<R> {
    pub(crate) fn new(binary: PathBuf, api_key: ApiKey, runner: R) -> Self {
        Self {
            binary,
            api_key,
            runner,
        }
    }

    fn score(&self, unit: &JudgedUnit, rule: &JudgementRule) -> Result<Judgement> {
        let stdout = self.runner.run(&self.binary, &arguments(unit, rule))?;
        let score = parse_raw_score(&stdout)
            .with_context(|| format!("jev-lint rule {} on {}", rule.id, unit.path.display()))?;
        Ok(Judgement::Scored(score))
    }
}

impl<R: CommandRunner> JudgementBackend for JevLintBackend<R> {
    fn judge(&self, unit: &JudgedUnit, rule: &JudgementRule) -> Result<Judgement> {
        match self.api_key {
            ApiKey::Absent => Ok(Judgement::Unjudged(UnjudgedReason::MissingApiKey)),
            ApiKey::Present => self.score(unit, rule),
        }
    }
}

/// `--loose` makes jev-lint list scores that are under its own cutoff, so the raw score is always
/// reported and the cutoff stays a decision of the caller.
fn arguments(unit: &JudgedUnit, rule: &JudgementRule) -> Vec<OsString> {
    vec![
        OsString::from("run"),
        OsString::from(&rule.id),
        unit.path.clone().into_os_string(),
        OsString::from("--loose"),
        OsString::from("--file"),
        rule.file.clone().into_os_string(),
    ]
}

/// Reads the number after the first `score` in the output. The README shows findings as
/// `score 0.89`; stage 3F confirms the exact layout against a real run.
fn parse_raw_score(stdout: &str) -> Result<RawScore> {
    let Some((_, after)) = stdout.split_once("score") else {
        bail!("no score in jev-lint output");
    };
    let number: String = after
        .trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '=')
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value = number
        .parse::<f64>()
        .with_context(|| format!("jev-lint score is not a number: {number:?}"))?;
    RawScore::parse(value)
}
