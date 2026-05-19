from pathlib import Path

from tests.python.conftest import load_script_module


collect_skills = load_script_module(
    "agents/skills/skill-auditor/scripts/collect_skills.py",
    "skill_auditor_collect_skills",
)


def write_skill(path: Path, name: str) -> None:
    path.mkdir(parents=True, exist_ok=True)
    (path / "SKILL.md").write_text(
        f"---\nname: {name}\ndescription: Use when testing {name}.\n---\n\n# {name}\n",
        encoding="utf-8",
    )


def test_codex_default_skill_dirs_include_stable_codex_locations() -> None:
    dirs = collect_skills.default_skill_dirs("codex")

    assert "~/.codex/skills" in dirs
    assert "~/.codex/plugins/cache" in dirs
    assert "~/.codex/vendor_imports/skills" in dirs
    assert "~/.codex/.tmp/plugins" not in dirs


def test_collect_uses_explicit_skill_dirs_for_codex_provider(tmp_path: Path) -> None:
    write_skill(tmp_path / "codex-skill", "codex-skill")

    result = collect_skills.collect(
        skill_dirs=[str(tmp_path)],
        provider="codex",
        include_project_skills=False,
    )

    assert result["summary"]["total_skills"] == 1
    assert result["skills"][0]["name"] == "codex-skill"
    assert result["skills"][0]["scope"] == "global"
