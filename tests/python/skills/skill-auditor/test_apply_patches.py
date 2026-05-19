import json
from pathlib import Path

from tests.python.conftest import load_script_module


apply_patches = load_script_module(
    "agents/skills/skill-auditor/scripts/apply_patches.py",
    "apply_patches",
)


def test_load_patches_reads_patch_json_files_in_sorted_order(tmp_path: Path) -> None:
    (tmp_path / "b.patch.json").write_text(json.dumps({"skill_name": "b"}), encoding="utf-8")
    (tmp_path / "a.patch.json").write_text(json.dumps({"skill_name": "a"}), encoding="utf-8")
    (tmp_path / "ignored.json").write_text(json.dumps({"skill_name": "ignored"}), encoding="utf-8")

    patches = apply_patches.load_patches(str(tmp_path))

    assert [patch["skill_name"] for patch in patches] == ["a", "b"]
    assert all(patch["_source_file"].endswith(".patch.json") for patch in patches)


def test_apply_description_patch_reports_preview_in_dry_run(tmp_path: Path) -> None:
    skill = tmp_path / "SKILL.md"
    skill.write_text("---\nname: example\ndescription: Old text.\n---\n# Body\n", encoding="utf-8")

    result = apply_patches.apply_description_patch(
        str(skill),
        current_description="Old text.",
        proposed_description="New text for routing.",
        dry_run=True,
    )

    assert result["status"] == "dry_run"
    assert result["preview"] == "description: >\n  New text for routing."
    assert "Old text." in skill.read_text(encoding="utf-8")


def test_apply_description_patch_updates_file_and_creates_backup(tmp_path: Path) -> None:
    skill = tmp_path / "SKILL.md"
    skill.write_text("---\nname: example\ndescription: Old text.\n---\n# Body\n", encoding="utf-8")

    result = apply_patches.apply_description_patch(
        str(skill),
        current_description="Old text.",
        proposed_description="New text.",
        dry_run=False,
        backup=True,
    )

    assert result["status"] == "applied"
    assert "description: >\n  New text.\n" in skill.read_text(encoding="utf-8")
    assert (tmp_path / "SKILL.md.bak").is_file()
