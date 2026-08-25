#!/bin/bash

# lint_format_gha.sh
# Quality Loop: lint GitHub Actions workflows via actionlint; emit residual
# diagnostics as PostToolUse additionalContext JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

case "$FILE_PATH" in
  */.github/workflows/*.yml | */.github/workflows/*.yaml) ;;
  *)
    exit 0
    ;;
esac

require_cmd actionlint
VIOLATIONS=""
if ! VIOLATIONS=$(actionlint -oneline "$FILE_PATH" 2>&1); then
  emit_post_tool_context "actionlint" "$VIOLATIONS"
fi
