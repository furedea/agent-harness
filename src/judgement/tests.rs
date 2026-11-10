use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::jev_lint::{ApiKey, CommandOutput, CommandRunner, JevLintBackend};
use super::{JudgedUnit, Judgement, JudgementBackend, JudgementRule, RawScore, UnjudgedReason};

struct FakeRunner {
    output: CommandOutput,
    calls: RefCell<Vec<(PathBuf, Vec<OsString>)>>,
}

impl FakeRunner {
    fn replying(stdout: &str) -> Self {
        Self::failing(stdout, "exit status: 0", "")
    }

    fn failing(stdout: &str, status: &str, stderr: &str) -> Self {
        Self {
            output: CommandOutput {
                stdout: stdout.to_owned(),
                status: status.to_owned(),
                stderr: stderr.to_owned(),
            },
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl CommandRunner for &FakeRunner {
    fn run(&self, program: &Path, args: &[OsString]) -> Result<CommandOutput> {
        self.calls
            .borrow_mut()
            .push((program.to_path_buf(), args.to_vec()));
        Ok(self.output.clone())
    }
}

fn score_of(stdout: &str, unit: &JudgedUnit, rule: &JudgementRule) -> Judgement {
    let runner = FakeRunner::replying(stdout);
    backend(&runner, ApiKey::Present).judge(unit, rule).unwrap()
}

/// The interface is usable without jev-lint: callers can substitute any backend.
struct FixedBackend(Judgement);

impl JudgementBackend for FixedBackend {
    fn judge(&self, _unit: &JudgedUnit, _rule: &JudgementRule) -> Result<Judgement> {
        Ok(self.0)
    }
}

fn unit() -> JudgedUnit {
    JudgedUnit {
        path: PathBuf::from("src/lib.py"),
    }
}

fn rule() -> JudgementRule {
    JudgementRule {
        id: "python/fn-name-promises".to_owned(),
        version: 1,
        file: PathBuf::from("rules/fn_name_promises.yml"),
    }
}

fn backend(runner: &FakeRunner, api_key: ApiKey) -> JevLintBackend<&FakeRunner> {
    JevLintBackend::new(
        PathBuf::from("/nix/store/jev-lint/bin/jev-lint"),
        api_key,
        runner,
    )
}

#[test]
fn raw_score_accepts_the_closed_unit_interval() {
    for value in [0.0, 0.5, 1.0] {
        let score = RawScore::parse(value).unwrap().value();
        assert!((score - value).abs() < f64::EPSILON, "{value}");
    }
}

#[test]
fn raw_score_rejects_values_outside_the_unit_interval() {
    for value in [-0.01, 1.01, f64::NAN] {
        assert!(RawScore::parse(value).is_err(), "{value}");
    }
}

#[test]
fn judging_without_an_api_key_is_unjudged_and_does_not_run_the_tool() {
    let runner = FakeRunner::replying("score 0.9");

    let judgement = backend(&runner, ApiKey::Absent)
        .judge(&unit(), &rule())
        .unwrap();

    assert_eq!(
        judgement,
        Judgement::Unjudged(UnjudgedReason::MissingApiKey)
    );
    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn judging_with_an_api_key_runs_the_rule_on_the_unit_and_returns_the_raw_score() {
    let runner =
        FakeRunner::replying("src/lib.py:3 rule python/fn-name-promises score 0.89 cutoff 0.56");

    let judgement = backend(&runner, ApiKey::Present)
        .judge(&unit(), &rule())
        .unwrap();

    assert_eq!(judgement, Judgement::Scored(RawScore::parse(0.89).unwrap()));
    let calls = runner.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0,
        PathBuf::from("/nix/store/jev-lint/bin/jev-lint")
    );
    let expected: Vec<OsString> = [
        "run",
        "python/fn-name-promises",
        "src/lib.py",
        "--loose",
        "--file",
        "rules/fn_name_promises.yml",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    assert_eq!(calls[0].1, expected);
}

#[test]
fn score_separators_are_tolerated() {
    for stdout in ["score: 0.4", "score=0.4", "score   0.4"] {
        let runner = FakeRunner::replying(stdout);
        let judgement = backend(&runner, ApiKey::Present)
            .judge(&unit(), &rule())
            .unwrap();
        assert_eq!(judgement, Judgement::Scored(RawScore::parse(0.4).unwrap()));
    }
}

#[test]
fn output_without_a_usable_score_is_an_error_not_a_zero() {
    for stdout in ["", "no findings", "score high", "score 7.5"] {
        let runner = FakeRunner::replying(stdout);
        assert!(
            backend(&runner, ApiKey::Present)
                .judge(&unit(), &rule())
                .is_err(),
            "{stdout:?}"
        );
    }
}

#[test]
fn a_path_or_rule_id_that_contains_score_is_not_read_as_the_score() {
    let unit = JudgedUnit {
        path: PathBuf::from("scores/score.py"),
    };
    let rule = JudgementRule {
        id: "python/score-names-describe-values".to_owned(),
        ..rule()
    };
    let stdout = "scores/score.py:3 rule python/score-names-describe-values score 0.61 cutoff 0.5";

    assert_eq!(
        score_of(stdout, &unit, &rule),
        Judgement::Scored(RawScore::parse(0.61).unwrap())
    );
}

#[test]
fn several_findings_for_one_unit_report_the_highest_score() {
    let stdout = "src/lib.py:3 score 0.31\nsrc/lib.py:9 score 0.88\nsrc/lib.py:20 score 0.52";

    assert_eq!(
        score_of(stdout, &unit(), &rule()),
        Judgement::Scored(RawScore::parse(0.88).unwrap())
    );
}

#[test]
fn a_missing_score_error_carries_the_exit_status_and_the_end_of_stderr() {
    let runner = FakeRunner::failing("", "exit status: 2", "line1\nline2\nrule file is invalid");

    let error = backend(&runner, ApiKey::Present)
        .judge(&unit(), &rule())
        .unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("exit status: 2"), "{message}");
    assert!(message.contains("rule file is invalid"), "{message}");
}

#[test]
fn a_replacement_backend_satisfies_the_interface() {
    let fixed = FixedBackend(Judgement::Unjudged(UnjudgedReason::MissingApiKey));

    assert_eq!(
        fixed.judge(&unit(), &rule()).unwrap(),
        Judgement::Unjudged(UnjudgedReason::MissingApiKey)
    );
}
