#!/bin/bash

# lint_format_py.sh
# Quality Loop: format -> auto-fix -> emit residual violations as PostToolUse
# additionalContext JSON. Always exits 0; cross-file checks live in pre-commit / CI.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

if PROJECT_DIR=$(find_project_root "$(dirname "$FILE_PATH")" pyproject.toml uv.lock); then
  require_cmd uv
  cd "$PROJECT_DIR"
  uv run --frozen ruff format "$FILE_PATH" >/dev/null 2>&1 || true
  uv run --frozen ruff check --fix-only --quiet "$FILE_PATH" >/dev/null 2>&1 || true
  VIOLATIONS=$(uv run --frozen ruff check --output-format=concise --quiet "$FILE_PATH" 2>&1 || true)
else
  require_cmd ruff
  ruff format "$FILE_PATH" >/dev/null 2>&1 || true
  ruff check --fix-only --quiet "$FILE_PATH" >/dev/null 2>&1 || true
  VIOLATIONS=$(ruff check --output-format=concise --quiet "$FILE_PATH" 2>&1 || true)
fi

emit_post_tool_context "ruff" "$VIOLATIONS"
