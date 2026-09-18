"""Hermes plugin bridging Hermes Agent lifecycle events to agent-harness hooks.

Forwards each registered event to ~/.hermes/hooks/hook_adapter.py, which
matches it against ~/.hermes/hooks.json and runs the configured commands.
The adapter's blocking output is translated back into Hermes directives.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable

_ADAPTER = Path(
    os.environ.get(
        "AGENT_HARNESS_HERMES_ADAPTER",
        str(Path.home() / ".hermes" / "hooks" / "hook_adapter.py"),
    )
)
_TIMEOUT_SECONDS = int(os.environ.get("AGENT_HARNESS_HOOK_TIMEOUT", "30"))

_EVENTS = [
    "pre_tool_call",
    "post_tool_call",
    "pre_verify",
    "on_session_start",
    "on_session_end",
    "on_session_finalize",
    "on_session_reset",
    "subagent_start",
    "subagent_stop",
    "pre_approval_request",
    "post_approval_response",
]


def _dispatch(event_name: str, kwargs: dict[str, Any]) -> dict[str, Any] | None:
    if not _ADAPTER.is_file():
        return None
    payload = {"hook_event_name": event_name, **kwargs}
    try:
        result = subprocess.run(
            [sys.executable, str(_ADAPTER), "dispatch", event_name],
            input=json.dumps(payload, default=str),
            capture_output=True,
            text=True,
            timeout=_TIMEOUT_SECONDS,
            check=False,
        )
    except Exception:
        return None
    return _directive(event_name, result)


def _directive(event_name: str, result: subprocess.CompletedProcess) -> dict[str, Any] | None:
    if result.returncode == 2:
        message = result.stderr.strip() or "blocked by agent-harness hook"
        return _blocking_directive(event_name, message)
    for line in reversed(result.stdout.splitlines()):
        data = _parse_json(line)
        if data is None:
            continue
        if data.get("action") in ("block", "approve") and event_name == "pre_tool_call":
            return data
        if data.get("decision") == "block" and data.get("reason"):
            return _blocking_directive(event_name, data["reason"])
    return None


def _blocking_directive(event_name: str, message: str) -> dict[str, Any]:
    if event_name == "pre_verify":
        return {"action": "continue", "message": message}
    return {"action": "block", "message": message}


def _parse_json(line: str) -> dict[str, Any] | None:
    line = line.strip()
    if not line.startswith("{"):
        return None
    try:
        data = json.loads(line)
    except ValueError:
        return None
    return data if isinstance(data, dict) else None


def register(ctx: Any) -> None:
    for event_name in _EVENTS:
        ctx.register_hook(event_name, _make_handler(event_name))


def _make_handler(event_name: str) -> Callable[..., dict[str, Any] | None]:
    def _hook(**kwargs: Any) -> dict[str, Any] | None:
        return _dispatch(event_name, kwargs)

    return _hook
