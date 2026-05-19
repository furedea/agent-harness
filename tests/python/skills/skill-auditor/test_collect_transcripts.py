import json
from pathlib import Path

from tests.python.conftest import load_script_module


collect_transcripts = load_script_module(
    "agents/skills/skill-auditor/scripts/collect_transcripts.py",
    "skill_auditor_collect_transcripts",
)


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n",
        encoding="utf-8",
    )


def test_codex_provider_collects_turns_and_skill_loads_for_matching_cwd(tmp_path: Path) -> None:
    session_root = tmp_path / "sessions"
    project_path = tmp_path / "project"
    other_project_path = tmp_path / "other"
    skill_path = tmp_path / "skills" / "python-style" / "SKILL.md"

    write_jsonl(
        session_root / "2026" / "05" / "04" / "rollout-a.jsonl",
        [
            {
                "timestamp": "2026-05-04T09:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": "session-a",
                    "timestamp": "2026-05-04T09:00:00Z",
                    "cwd": str(project_path),
                },
            },
            {
                "timestamp": "2026-05-04T09:01:00Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Python code を書いて"}],
                },
            },
            {
                "timestamp": "2026-05-04T09:02:00Z",
                "type": "response_item",
                "payload": {
                    "type": "function_call",
                    "name": "exec_command",
                    "arguments": json.dumps({"cmd": f"sed -n '1,20p' {skill_path}"}),
                },
            },
        ],
    )
    write_jsonl(
        session_root / "2026" / "05" / "04" / "rollout-b.jsonl",
        [
            {
                "timestamp": "2026-05-04T09:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": "session-b",
                    "timestamp": "2026-05-04T09:00:00Z",
                    "cwd": str(other_project_path),
                },
            },
            {
                "timestamp": "2026-05-04T09:01:00Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "unrelated"}],
                },
            },
        ],
    )

    result = collect_transcripts.collect(
        str(project_path),
        days=0,
        provider="codex",
        session_root=str(session_root),
    )

    assert result["summary"]["total_sessions"] == 1
    session = result["sessions"][0]
    assert session["session_id"] == "session-a"
    assert session["project_dir"] == str(project_path)
    assert session["skills_loaded"] == [str(skill_path)]
    assert session["turn_skill_map"] == [
        {
            "turn_index": 0,
            "user_message": "Python code を書いて",
            "skills_loaded_after": [str(skill_path)],
            "is_builtin_command": False,
        }
    ]


def test_codex_provider_collects_all_sessions_recursively(tmp_path: Path) -> None:
    session_root = tmp_path / "sessions"
    project_a = tmp_path / "project-a"
    project_b = tmp_path / "project-b"

    for project_path, filename in (
        (project_a, "2026/05/04/rollout-a.jsonl"),
        (project_b, "2026/05/03/rollout-b.jsonl"),
    ):
        write_jsonl(
            session_root / filename,
            [
                {
                    "timestamp": "2026-05-04T09:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": filename,
                        "timestamp": "2026-05-04T09:00:00Z",
                        "cwd": str(project_path),
                    },
                },
                {
                    "timestamp": "2026-05-04T09:01:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "hello"}],
                    },
                },
            ],
        )

    result = collect_transcripts.collect(
        "all",
        days=0,
        provider="codex",
        session_root=str(session_root),
    )

    assert result["summary"]["total_sessions"] == 2
    assert result["summary"]["sessions_by_project"] == {
        str(project_a): 1,
        str(project_b): 1,
    }
