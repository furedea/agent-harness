import json
from pathlib import Path

from tests.python.conftest import load_script_module


generate_report = load_script_module(
    "agents/skills/skill-auditor/scripts/generate_report.py",
    "generate_report",
)


def test_load_json_safe_returns_none_for_missing_or_invalid_json(tmp_path: Path) -> None:
    invalid = tmp_path / "invalid.json"
    invalid.write_text("{", encoding="utf-8")

    assert generate_report.load_json_safe(str(tmp_path / "missing.json")) is None
    assert generate_report.load_json_safe(str(invalid)) is None


def test_generate_report_embeds_workspace_data_with_builtin_template(tmp_path: Path) -> None:
    (tmp_path / "audit_report.json").write_text(
        json.dumps({"meta": {"sessions_analyzed": 2}, "skill_reports": []}),
        encoding="utf-8",
    )
    (tmp_path / "health_history.json").write_text(json.dumps([{"score": "healthy"}]), encoding="utf-8")

    html = generate_report.generate_report(str(tmp_path), template_path=None)

    assert "const REPORT_DATA =" in html
    assert '"sessions_analyzed": 2' in html
    assert '"health_history": [' in html


def test_generate_report_uses_custom_template(tmp_path: Path) -> None:
    template = tmp_path / "template.html"
    template.write_text("<script>/*__EMBEDDED_DATA__*/</script>", encoding="utf-8")

    html = generate_report.generate_report(str(tmp_path), template_path=str(template))

    assert html.startswith("<script>const REPORT_DATA = ")
    assert '"audit_report": null' in html
