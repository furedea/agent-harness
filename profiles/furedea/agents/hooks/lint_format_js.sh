#!/bin/bash

# lint_format_js.sh
# Quality Loop: format (oxfmt) -> auto-fix (oxlint --fix) -> emit residual
# warnings/errors as PostToolUse additionalContext JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd oxfmt
oxfmt --write "$FILE_PATH" >/dev/null 2>&1 || true

require_cmd oxlint
oxlint --fix "$FILE_PATH" >/dev/null 2>&1 || true

VIOLATIONS=""
if ! VIOLATIONS=$(oxlint --deny-warnings "$FILE_PATH" 2>&1); then
  emit_post_tool_context "oxlint" "$VIOLATIONS"
fi
