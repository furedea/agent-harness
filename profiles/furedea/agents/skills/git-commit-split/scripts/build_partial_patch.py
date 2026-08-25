#!/usr/bin/env python3
"""Filter a unified git diff to a selected subset of hunks.

The output is a valid diff suitable for ``git apply --cached``. Selection is
specified as JSON, either inline or as a path to a .json file.

Usage:
    build_partial_patch.py <diff_file> <selection>

Selection schema (JSON array):
    [
      {"file": "path/to/file.py", "hunks": [1, 3]},   # 1-based hunk indices
      {"file": "path/to/other.py", "hunks": "all"}    # whole-file
    ]

Hunks are numbered 1..N per file, in the order they appear in the input diff.
A binary diff or a pure rename without content hunks must use ``"hunks": "all"``.

Exit codes:
    0  success
    1  IO/parse error
    2  selection references an unknown file or hunk
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


_HUNK_HEADER = re.compile(r"^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@")
_FILE_HEADER = re.compile(r"^diff --git ")


def parse_diff(text: str) -> list[dict]:
    """Split a unified diff into one entry per file.

    Each entry has:
        path:    target path (the ``b/`` side)
        header:  list of lines before the first hunk (diff/index/---/+++/...)
        hunks:   list of hunks; each hunk is a list of lines starting with ``@@``
    """
    files: list[dict] = []
    current: dict | None = None
    in_hunk = False
    for line in text.splitlines(keepends=True):
        if _FILE_HEADER.match(line):
            current = {"path": _extract_path(line), "header": [line], "hunks": []}
            files.append(current)
            in_hunk = False
            continue
        if current is None:
            continue
        if _HUNK_HEADER.match(line):
            current["hunks"].append([line])
            in_hunk = True
            continue
        if in_hunk:
            current["hunks"][-1].append(line)
        else:
            current["header"].append(line)
    return files


def _extract_path(diff_git_line: str) -> str:
    # `diff --git <prefix>/<path> <prefix>/<path>` — prefix is `a`/`b` by default,
    # but user config (diff.mnemonicPrefix) can change it to `i`/`w`/`c`/`o`.
    # Take the last whitespace-separated token and strip everything up to the
    # first `/` so any prefix works.
    parts = diff_git_line.rstrip("\n").split(" ")
    last = parts[-1]
    slash = last.find("/")
    if slash >= 0:
        return last[slash + 1 :]
    raise ValueError(f"cannot parse path from: {diff_git_line!r}")


def build_partial(files: list[dict], selection: list[dict]) -> str:
    by_path = {f["path"]: f for f in files}
    out: list[str] = []
    for entry in selection:
        path = entry["file"]
        if path not in by_path:
            print(
                f"unknown file in selection: {path}\navailable: {sorted(by_path)}",
                file=sys.stderr,
            )
            sys.exit(2)
        f = by_path[path]
        out.extend(f["header"])
        which = entry["hunks"]
        if which == "all":
            chosen = list(range(1, len(f["hunks"]) + 1))
        else:
            chosen = list(which)
        for idx in chosen:
            if idx < 1 or idx > len(f["hunks"]):
                print(
                    f"hunk {idx} not present in {path} (have {len(f['hunks'])})",
                    file=sys.stderr,
                )
                sys.exit(2)
            out.extend(f["hunks"][idx - 1])
    return "".join(out)


def _load_selection(arg: str) -> list[dict]:
    p = Path(arg)
    if p.is_file():
        return json.loads(p.read_text())
    return json.loads(arg)


def main() -> None:
    if len(sys.argv) != 3:
        sys.stderr.write(__doc__ or "")
        sys.exit(1)
    diff_text = Path(sys.argv[1]).read_text()
    selection = _load_selection(sys.argv[2])
    files = parse_diff(diff_text)
    sys.stdout.write(build_partial(files, selection))


if __name__ == "__main__":
    main()
