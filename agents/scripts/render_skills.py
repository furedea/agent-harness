#!/usr/bin/env python3
"""Render provider-specific Agent Skills from common SKILL.md sources."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import shutil
from collections.abc import Mapping
from typing import Any


COMMON_KEYS = {"name", "description"}
PROVIDERS = ("claude", "codex")
KEY_PATTERN = re.compile(r"^([A-Za-z0-9_-]+):")


def main() -> None:
    args = parse_args()
    overrides = json.loads(args.overrides.read_text())
    render_all(args.source, args.output, overrides)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Render provider-specific skills from common Agent Skills sources.",
    )
    parser.add_argument("--source", type=pathlib.Path, required=True)
    parser.add_argument("--overrides", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    return parser.parse_args()


def render_all(
    source_dir: pathlib.Path,
    output_dir: pathlib.Path,
    overrides: Mapping[str, Mapping[str, Any]],
) -> None:
    for skill_dir in sorted(path for path in source_dir.iterdir() if path.is_dir()):
        skill_name = skill_dir.name
        source_skill = skill_dir / "SKILL.md"
        if not source_skill.exists():
            continue

        frontmatter, body = split_frontmatter(source_skill.read_text())
        common_entries = common_frontmatter_entries(source_skill, frontmatter)
        skill_overrides = overrides.get(skill_name, {})
        frontmatter_overrides = skill_overrides.get("frontmatter", {})
        file_overrides = skill_overrides.get("files", {})

        for provider in PROVIDERS:
            provider_dir = output_dir / provider / "skills" / skill_name
            copy_support_files(skill_dir, provider_dir)
            provider_frontmatter = frontmatter_overrides.get(provider, {})
            provider_skill = render_skill(common_entries, provider_frontmatter, body)
            (provider_dir / "SKILL.md").write_text(provider_skill)
            write_extra_files(provider_dir, file_overrides.get(provider, {}))


def split_frontmatter(content: str) -> tuple[str, str]:
    if not content.startswith("---\n"):
        raise ValueError("SKILL.md must start with YAML frontmatter")

    marker = "\n---\n"
    end = content.find(marker, len("---\n"))
    if end == -1:
        raise ValueError("SKILL.md frontmatter must end with ---")

    return content[len("---\n") : end], content[end + len(marker) :]


def common_frontmatter_entries(path: pathlib.Path, frontmatter: str) -> list[str]:
    entries = split_entries(frontmatter)
    unknown_keys = [key for key, _entry in entries if key not in COMMON_KEYS]
    if unknown_keys:
        names = ", ".join(sorted(unknown_keys))
        raise ValueError(f"{path}: non-common frontmatter keys must move to nix/agents/skills.nix: {names}")
    return [entry for _key, entry in entries]


def split_entries(frontmatter: str) -> list[tuple[str, str]]:
    entries: list[tuple[str, list[str]]] = []
    current_key: str | None = None
    current_lines: list[str] = []

    for line in frontmatter.splitlines():
        match = KEY_PATTERN.match(line)
        if match:
            if current_key is not None:
                entries.append((current_key, current_lines))
            current_key = match.group(1)
            current_lines = [line]
            continue

        if current_key is None:
            if line.strip():
                raise ValueError(f"frontmatter line is not under a key: {line}")
            continue
        current_lines.append(line)

    if current_key is not None:
        entries.append((current_key, current_lines))

    return [(key, "\n".join(lines).rstrip()) for key, lines in entries]


def copy_support_files(source_dir: pathlib.Path, provider_dir: pathlib.Path) -> None:
    if provider_dir.exists():
        shutil.rmtree(provider_dir)
    provider_dir.mkdir(parents=True)

    for source_path in source_dir.iterdir():
        if source_path.name == "SKILL.md":
            continue
        target_path = provider_dir / source_path.name
        if source_path.is_dir():
            shutil.copytree(source_path, target_path, symlinks=True)
        else:
            shutil.copy2(source_path, target_path)

    grant_user_write(provider_dir)


def grant_user_write(root: pathlib.Path) -> None:
    """Ensure copied files retain user write permission.

    `shutil.copy*` preserves the source mode, so when the source lives in
    `/nix/store` (read-only), the destination inherits 0o555 / 0o444 and any
    later `write_extra_files` step fails with PermissionError.
    """
    for path in (root, *root.rglob("*")):
        if path.is_symlink():
            continue
        try:
            mode = path.stat().st_mode
        except OSError:
            continue
        path.chmod(mode | 0o200)


def render_skill(
    common_entries: list[str],
    overrides: Mapping[str, Any],
    body: str,
) -> str:
    frontmatter = [*common_entries]
    for key in sorted(overrides):
        frontmatter.append(frontmatter_entry(key, overrides[key]))
    return "---\n" + "\n".join(frontmatter).rstrip() + "\n---\n" + body


def frontmatter_entry(key: str, value: Any) -> str:
    if isinstance(value, bool):
        rendered = "true" if value else "false"
        return f"{key}: {rendered}"
    if isinstance(value, str):
        return f"{key}: {json.dumps(value, ensure_ascii=False)}"
    if isinstance(value, list):
        return f"{key}: {json.dumps(value, ensure_ascii=False)}"
    raise TypeError(f"unsupported frontmatter value for {key}: {value!r}")


def write_extra_files(provider_dir: pathlib.Path, files: Mapping[str, str]) -> None:
    for relative_path, content in files.items():
        target = resolve_extra_file_path(provider_dir, relative_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)


def resolve_extra_file_path(provider_dir: pathlib.Path, relative_path: str) -> pathlib.Path:
    candidate = pathlib.PurePosixPath(relative_path)
    if candidate.is_absolute() or any(part == ".." for part in candidate.parts):
        raise ValueError(f"extra file path must be relative and stay within the skill: {relative_path}")
    return provider_dir.joinpath(*candidate.parts)


if __name__ == "__main__":
    main()
