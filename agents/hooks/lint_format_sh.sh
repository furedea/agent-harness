#!/bin/bash

# lint_format_sh.sh
# Quality Loop: format (shfmt -w) -> emit residual shellcheck diagnostics as
# PostToolUse additionalContext JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd shfmt
shfmt -w "$FILE_PATH" >/dev/null 2>&1 || true

require_cmd shellcheck
VIOLATIONS=""
if ! VIOLATIONS=$(shellcheck -x -P SCRIPTDIR "$FILE_PATH" 2>&1); then
  emit_post_tool_context "shellcheck" "$VIOLATIONS"
fi
