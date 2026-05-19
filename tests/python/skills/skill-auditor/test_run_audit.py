from pathlib import Path

from tests.python.conftest import load_script_module


run_audit = load_script_module(
    "agents/skills/skill-auditor/scripts/run_audit.py",
    "skill_auditor_run_audit",
)


def test_default_base_dir_uses_user_provider_store_with_project_slug(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    project = repo / "agents" / "skills" / "skill-auditor"
    project.mkdir(parents=True)
    (repo / ".git").mkdir()

    base_dir = run_audit.default_base_dir("codex", str(project))

    expected_slug = f"{repo.parent.name}-repo-agents-skills-skill-auditor"
    assert base_dir == Path("~/.codex/skill-report").expanduser() / "projects" / expected_slug


def test_default_base_dir_uses_root_slug_for_repo_root(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / ".git").mkdir()

    base_dir = run_audit.default_base_dir("codex", str(repo))

    expected_slug = f"{repo.parent.name}-repo"
    assert base_dir == Path("~/.codex/skill-report").expanduser() / "projects" / expected_slug


def test_build_batches_marks_disable_model_invocation_skills() -> None:
    transcripts = {
        "sessions": [
            {"project_dir": "/tmp/project"},
            {"project_dir": "/tmp/project"},
        ]
    }
    manifest = {
        "skills": [
            {
                "name": "global-skill",
                "scope": "global",
                "disable_model_invocation": False,
            },
            {
                "name": "explicit-skill",
                "scope": "global",
                "disable_model_invocation": True,
            },
        ]
    }

    batches = run_audit.build_batches(transcripts, manifest, batch_size=60, max_batches=12)

    assert batches == [
        {
            "session_indices": [0, 1],
            "label": "global-only (mixed projects)",
            "visible_skill_names": ["global-skill", "explicit-skill"],
            "batch_index": 0,
            "dmi_skill_names": ["explicit-skill"],
        }
    ]


def test_merge_audit_reports_recalculates_skill_accuracy() -> None:
    reports = [
        {
            "skill_reports": [
                {
                    "skill_name": "python-style",
                    "skill_path": "/skills/python-style/SKILL.md",
                    "description_excerpt": "Python",
                    "stats": {
                        "total_fires": 1,
                        "correct_fires": 1,
                        "false_positives": 0,
                        "false_negatives": 0,
                        "accuracy": 1.0,
                    },
                    "incidents": [],
                    "health_assessment": "Healthy",
                    "suggested_fix": None,
                }
            ],
            "skills_never_fired": [],
            "competition_pairs": [],
            "coverage_gaps": [],
            "meta": {
                "sessions_analyzed": 1,
                "turns_analyzed": 2,
                "turns_with_skill_activity": 1,
                "turns_no_skill_needed": 1,
                "skills_in_scope": 1,
            },
        },
        {
            "skill_reports": [
                {
                    "skill_name": "python-style",
                    "skill_path": "/skills/python-style/SKILL.md",
                    "description_excerpt": "Python",
                    "stats": {
                        "total_fires": 1,
                        "correct_fires": 0,
                        "false_positives": 1,
                        "false_negatives": 0,
                        "accuracy": 0.0,
                    },
                    "incidents": [{"verdict": "false_positive"}],
                    "health_assessment": "Needs attention",
                    "suggested_fix": "Add exclusion.",
                }
            ],
            "skills_never_fired": [],
            "competition_pairs": [],
            "coverage_gaps": [],
            "meta": {
                "sessions_analyzed": 1,
                "turns_analyzed": 3,
                "turns_with_skill_activity": 1,
                "turns_no_skill_needed": 2,
                "skills_in_scope": 1,
            },
        },
    ]

    merged = run_audit.merge_audit_reports(reports)

    report = merged["skill_reports"][0]
    assert report["stats"]["total_fires"] == 2
    assert report["stats"]["correct_fires"] == 1
    assert report["stats"]["accuracy"] == 0.5
    assert report["suggested_fix"] == "Add exclusion."
    assert merged["meta"]["turns_analyzed"] == 5


def test_write_agent_prompts_creates_codex_reusable_prompts(tmp_path: Path) -> None:
    batches = [
        {
            "batch_index": 0,
            "session_indices": [0],
            "visible_skill_names": ["python-style"],
            "dmi_skill_names": [],
        }
    ]

    run_audit.write_agent_prompts(tmp_path, batches, "Japanese")

    prompt = tmp_path / "agent-prompts" / "routing_batch_0.md"
    assert prompt.is_file()
    assert "Write all human-readable output text in Japanese." in prompt.read_text(encoding="utf-8")
    assert (tmp_path / "agent-prompts" / "portfolio_analysis.md").is_file()
    assert (tmp_path / "agent-prompts" / "improvement_plan.md").is_file()
