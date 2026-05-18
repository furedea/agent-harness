#!/usr/bin/env python3
"""Merge managed Codex config keys while preserving Codex-owned state."""

from __future__ import annotations

import argparse
import pathlib
import tempfile
import tomllib
from collections.abc import Mapping, Sequence
from typing import Any


MANAGED_KEYS = (
    "model",
    "model_reasoning_effort",
    "personality",
    "approval_policy",
    "sandbox_mode",
    "approvals_reviewer",
    "notice",
    "tui",
    "plugins",
    "features",
    "default_permissions",
    "permissions",
)


def main() -> None:
    args = parse_args()
    source = load_toml(args.source)
    target = load_toml(args.target) if args.target.exists() else {}
    merged = merge_config(source, target)
    write_toml(args.target, merged)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Merge managed Codex config keys into a mutable config.toml.",
    )
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("target", type=pathlib.Path)
    return parser.parse_args()


def load_toml(path: pathlib.Path) -> dict[str, Any]:
    with path.open("rb") as file:
        return tomllib.load(file)


def merge_config(source: Mapping[str, Any], target: Mapping[str, Any]) -> dict[str, Any]:
    merged = dict(target)
    for key in MANAGED_KEYS:
        if key in source:
            merged[key] = source[key]
        else:
            merged.pop(key, None)
    return merged


def write_toml(path: pathlib.Path, data: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)

    content = toml_document(data)
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False) as file:
        temp_path = pathlib.Path(file.name)
        file.write(content)
    temp_path.replace(path)


def toml_document(data: Mapping[str, Any]) -> str:
    lines: list[str] = []
    scalar_items: dict[str, Any] = {}
    table_items: dict[str, Mapping[str, Any]] = {}

    for key, value in data.items():
        if isinstance(value, Mapping):
            table_items[key] = value
        else:
            scalar_items[key] = value

    for key, value in scalar_items.items():
        lines.append(f"{toml_key(key)} = {toml_value(value)}")

    for key, value in table_items.items():
        if lines:
            lines.append("")
        append_table(lines, [key], value)

    return "\n".join(lines) + "\n"


def append_table(lines: list[str], path: list[str], table: Mapping[str, Any]) -> None:
    scalar_items: dict[str, Any] = {}
    table_items: dict[str, Mapping[str, Any]] = {}

    for key, value in table.items():
        if isinstance(value, Mapping):
            table_items[key] = value
        else:
            scalar_items[key] = value

    if scalar_items:
        lines.append(f"[{'.'.join(toml_key(key) for key in path)}]")
        for key, value in scalar_items.items():
            lines.append(f"{toml_key(key)} = {toml_value(value)}")

    for key, value in table_items.items():
        if lines and lines[-1] != "":
            lines.append("")
        append_table(lines, [*path, key], value)


def toml_key(key: str) -> str:
    if key.replace("_", "").replace("-", "").isalnum() and not key[0].isdigit():
        return key
    return toml_string(key)


def toml_value(value: Any) -> str:
    if isinstance(value, str):
        return toml_string(value)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int | float):
        return str(value).lower()
    if isinstance(value, Sequence) and not isinstance(value, str | bytes | bytearray):
        return "[\n" + "".join(f"  {toml_value(item)},\n" for item in value) + "]"
    raise TypeError(f"Unsupported TOML value: {value!r}")


def toml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")
    return f'"{escaped}"'


if __name__ == "__main__":
    main()
