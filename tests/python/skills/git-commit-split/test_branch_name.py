import pytest

from tests.python.conftest import load_script_module


branch_name = load_script_module("agents/skills/git-commit-split/scripts/branch_name.py", "branch_name")


def test_parse_subject_accepts_scope_and_breaking_marker() -> None:
    assert branch_name.parse_subject("feat(api)!: add refresh token") == ("feat", "add refresh token")


def test_kebab_normalizes_to_ascii_branch_slug() -> None:
    assert branch_name.kebab("Add café support & OAuth 2.0!") == "add-cafe-support-oauth-2-0"


def test_parse_subject_rejects_unknown_conventional_commit_type() -> None:
    with pytest.raises(ValueError, match="unknown Conventional Commits type"):
        branch_name.parse_subject("misc: update docs")


def test_kebab_rejects_subject_without_ascii_alphanumerics() -> None:
    with pytest.raises(ValueError, match="no ASCII alphanumeric"):
        branch_name.kebab("東京")
