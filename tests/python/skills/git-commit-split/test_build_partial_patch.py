import pytest

from tests.python.conftest import load_script_module


build_partial_patch = load_script_module(
    "agents/skills/git-commit-split/scripts/build_partial_patch.py",
    "build_partial_patch",
)


DIFF_TEXT = """diff --git a/app.py b/app.py
index 1111111..2222222 100644
--- a/app.py
+++ b/app.py
@@ -1,2 +1,2 @@
-old
+new
@@ -10,2 +10,2 @@
-before
+after
diff --git a/readme.md b/readme.md
index 3333333..4444444 100644
--- a/readme.md
+++ b/readme.md
@@ -1 +1 @@
-Hello
+Hi
"""


def test_parse_diff_splits_files_and_hunks() -> None:
    files = build_partial_patch.parse_diff(DIFF_TEXT)

    assert [file["path"] for file in files] == ["app.py", "readme.md"]
    assert len(files[0]["hunks"]) == 2
    assert len(files[1]["hunks"]) == 1


def test_build_partial_keeps_selected_hunks_only() -> None:
    files = build_partial_patch.parse_diff(DIFF_TEXT)

    patch = build_partial_patch.build_partial(files, [{"file": "app.py", "hunks": [2]}])

    assert "diff --git a/app.py b/app.py" in patch
    assert "@@ -1,2 +1,2 @@" not in patch
    assert "@@ -10,2 +10,2 @@" in patch
    assert "diff --git a/readme.md b/readme.md" not in patch


def test_build_partial_exits_when_selection_references_missing_file() -> None:
    files = build_partial_patch.parse_diff(DIFF_TEXT)

    with pytest.raises(SystemExit) as error:
        build_partial_patch.build_partial(files, [{"file": "missing.py", "hunks": "all"}])

    assert error.value.code == 2
