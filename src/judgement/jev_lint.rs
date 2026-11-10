//! `mizchi/jev-lint` as the judgement backend.
//!
//! The binary comes from the Nix package (`packages.<system>.jev-lint`), never from `npx`. The API
//! key is read only by the child process; this module checks that a key variable is set and never
//! reads, logs, or stores its value.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use super::{JudgedUnit, Judgement, JudgementBackend, JudgementRule, RawScore, UnjudgedReason};

const API_KEY_VARIABLES: [&str; 2] = ["TYPESAFE_API_KEY", "TYPESAFEAI_API_KEY"];
const STDERR_TAIL_LINES: usize = 3;

pub(crate) trait CommandRunner {
    /// Runs `program` and returns its standard output whatever the exit status is, because jev-lint
    /// may exit non-zero when it reports findings.
    fn run(&self, program: &Path, args: &[OsString]) -> Result<CommandOutput>;
}

#[derive(Debug, Clone)]
pub(crate) struct CommandOutput {
    pub(crate) stdout: String,
    pub(crate) status: String,
    pub(crate) stderr: String,
}

impl CommandOutput {
    /// The exit status and the last lines of stderr, to explain a run that produced no score.
    fn failure_summary(&self) -> String {
        let mut tail: Vec<&str> = self.stderr.lines().rev().take(STDERR_TAIL_LINES).collect();
        tail.reverse();
        format!("{}; stderr: {}", self.status, tail.join(" | "))
    }
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
    fn run(&self, program: &Path, args: &[OsString]) -> Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {}", program.display()))?;
        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
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
        let output = self.runner.run(&self.binary, &arguments(unit, rule))?;
        let score = parse_raw_score(&output.stdout)
            .with_context(|| failure_message(unit, rule, &output))?;
        Ok(Judgement::Scored(score))
    }
}

fn failure_message(unit: &JudgedUnit, rule: &JudgementRule, output: &CommandOutput) -> String {
    format!(
        "jev-lint rule {} on {} ({})",
        rule.id,
        unit.path.display(),
        output.failure_summary()
    )
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

/// The highest score among the findings in the output. The README shows a finding as
/// `score 0.89`; stage 3F confirms the exact layout against a real run.
fn parse_raw_score(stdout: &str) -> Result<RawScore> {
    let scores = score_texts(stdout)
        .filter_map(|text| text.parse::<f64>().ok())
        .map(RawScore::parse)
        .collect::<Result<Vec<_>>>()?;
    scores
        .into_iter()
        .max_by(|left, right| left.value().total_cmp(&right.value()))
        .ok_or_else(|| anyhow!("no score in jev-lint output"))
}

fn score_texts(stdout: &str) -> impl Iterator<Item = &str> {
    stdout.lines().flat_map(line_score_texts)
}

fn line_score_texts(line: &str) -> Vec<&str> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| score_text(token, tokens.get(index + 1).copied()))
        .collect()
}

/// The text after a `score` token, which must be the whole word: paths and rule ids that merely
/// contain "score" do not count. The value follows as `score 0.9`, `score: 0.9`, or `score=0.9`.
fn score_text<'a>(token: &'a str, next: Option<&'a str>) -> Option<&'a str> {
    let rest = token.strip_prefix("score")?;
    if !(rest.is_empty() || rest.starts_with([':', '='])) {
        return None;
    }
    let inline = rest.trim_start_matches([':', '=']);
    if inline.is_empty() {
        return next.map(|text| text.trim_start_matches([':', '=']));
    }
    Some(inline)
}
