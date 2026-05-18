#!/bin/bash

# lint_format_lua.sh
# Quality Loop: stylua -> capture residual selene diagnostics as PostToolUse
# additionalContext JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd stylua
stylua "$FILE_PATH" >/dev/null 2>&1 || true

require_cmd selene
# selene 0.25+ only reads selene.toml from CWD (no upward search); walk up manually.
SELENE_DIR=$(find_project_root "$(dirname "$FILE_PATH")" selene.toml) || SELENE_DIR="$(dirname "$FILE_PATH")"
VIOLATIONS=""
if ! VIOLATIONS=$( (cd "$SELENE_DIR" && selene "$FILE_PATH") 2>&1); then
  emit_post_tool_context "selene" "$VIOLATIONS"
fi
