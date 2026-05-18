#!/usr/bin/env python3
"""Generate a branch name from a Conventional Commits subject line.

The output is a kebab-case branch slug suitable for ``git switch -c``. The
shape is ``<type>/<kebab-subject>``; the optional commit scope is intentionally
dropped because branch names with parentheses or slashes inside the scope
(``feat(api/v2): ...``) confuse some tooling and add noise reviewers don't
read. Keep this in sync with ``references/conventional_commits.md`` and
``references/pr_per_feature_execute.md``.

Usage:
    branch_name.py <subject> [--avoid-existing]

Arguments:
    <subject>           A Conventional Commits subject like
                        ``feat(auth): add JWT refresh-token rotation``.
                        The leading ``<type>(<scope>)?(!)?:`` is required so
                        the type can be lifted out for the prefix.

Options:
    --avoid-existing    Probe local refs (``git branch --list``) and the
                        ``origin`` remote (``git ls-remote --heads origin``)
                        and append ``-2``, ``-3``, ... until the slug is
                        unused. Without this flag the script does not touch
                        git at all, so it is safe to call repeatedly.

Exit codes:
    0  success — slug printed to stdout
    1  git command failed while resolving collisions
    2  malformed subject (no recognized Conventional Commits type prefix)
"""

from __future__ import annotations

import re
import subprocess
import sys
import unicodedata

# Conventional Commits types this skill recognizes. Keep aligned with the
# table in ``references/conventional_commits.md``.
_VALID_TYPES = frozenset(
    {
        "feat",
        "fix",
        "refactor",
        "perf",
        "docs",
        "test",
        "build",
        "ci",
        "chore",
        "style",
        "revert",
    }
)

# `<type>(<scope>)?(!)?: <subject>` — scope and the breaking-change `!` are
# optional and ignored for branch naming.
_SUBJECT_RE = re.compile(r"^(?P<type>[a-z]+)(?:\([^)]*\))?!?:\s*(?P<subject>.+?)\s*$")

# Branch slugs longer than this start to wrap in `gh pr list` and most CI
# UIs. The cap is generous; real subjects rarely hit it.
_MAX_SLUG_LEN = 50


def parse_subject(line: str) -> tuple[str, str]:
    m = _SUBJECT_RE.match(line.strip())
    if not m:
        raise ValueError(
            f"not a Conventional Commits subject: {line!r}\n"
            "expected '<type>(<scope>)?: <subject>' with a recognized type"
        )
    t = m.group("type")
    if t not in _VALID_TYPES:
        raise ValueError(f"unknown Conventional Commits type: {t!r}\nvalid types: {sorted(_VALID_TYPES)}")
    return t, m.group("subject")


def kebab(subject: str) -> str:
    # Normalize accented chars (é → e), drop everything that isn't ASCII
    # alphanumeric, collapse separators, trim, and truncate. Branch refs that
    # contain non-ASCII break some hosts and most tab-completion setups, so
    # we strip rather than preserve.
    normalized = unicodedata.normalize("NFKD", subject)
    ascii_only = normalized.encode("ascii", "ignore").decode("ascii")
    slug = re.sub(r"[^a-z0-9]+", "-", ascii_only.lower()).strip("-")
    if len(slug) > _MAX_SLUG_LEN:
        # Cut at the previous word boundary so the slug stays readable;
        # falling back to a hard cut is fine if there's no `-` to break on.
        cut = slug[:_MAX_SLUG_LEN].rstrip("-")
        slug = cut.rsplit("-", 1)[0] if "-" in cut else cut
    if not slug:
        raise ValueError(f"subject contains no ASCII alphanumeric characters: {subject!r}")
    return slug


def _ref_in_use(name: str) -> bool:
    """True if ``name`` resolves to a local branch or to a head on ``origin``.

    A non-zero exit from ``git ls-remote`` (no remote, network down,
    permission issue) is treated as "can't tell" — we warn and fall back to
    the local check so the caller still gets a usable slug. The risk is a
    remote-only collision we miss; the caller will hit it again at push time
    and can rerun this script with ``--avoid-existing`` from inside the repo
    once the remote is reachable.
    """
    local = subprocess.run(
        ["git", "branch", "--list", name],
        capture_output=True,
        text=True,
        check=False,
    )
    if local.returncode != 0:
        raise RuntimeError(f"git branch --list failed: {local.stderr.strip()}")
    if local.stdout.strip():
        return True
    remote = subprocess.run(
        ["git", "ls-remote", "--heads", "origin", name],
        capture_output=True,
        text=True,
        check=False,
    )
    if remote.returncode != 0:
        print(
            f"warning: could not query origin (returncode {remote.returncode}); checked local refs only",
            file=sys.stderr,
        )
        return False
    return bool(remote.stdout.strip())


def avoid_collision(base: str) -> str:
    if not _ref_in_use(base):
        return base
    for n in range(2, 100):
        candidate = f"{base}-{n}"
        if not _ref_in_use(candidate):
            return candidate
    raise RuntimeError(f"could not find an unused suffix for {base!r}")


def main() -> None:
    args = sys.argv[1:]
    avoid = "--avoid-existing" in args
    args = [a for a in args if a != "--avoid-existing"]
    if len(args) != 1:
        sys.stderr.write(__doc__ or "")
        sys.exit(2)
    try:
        t, subject = parse_subject(args[0])
        slug = f"{t}/{kebab(subject)}"
    except ValueError as e:
        print(str(e), file=sys.stderr)
        sys.exit(2)
    if avoid:
        try:
            slug = avoid_collision(slug)
        except RuntimeError as e:
            print(str(e), file=sys.stderr)
            sys.exit(1)
    print(slug)


if __name__ == "__main__":
    main()
