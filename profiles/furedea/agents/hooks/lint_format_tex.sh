#!/bin/bash

# lint_format_tex.sh
# Quality Loop: tex-fmt -> capture residual chktex diagnostics as PostToolUse
# additionalContext JSON. Always exits 0.
# Handles .tex, .cls, .sty (format + lint) and .bib (format only).

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

EXTENSION="${FILE_PATH##*.}"

require_cmd tex-fmt
tex-fmt "$FILE_PATH" >/dev/null 2>&1 || true

# chktex does not support .bib files.
[ "$EXTENSION" = "bib" ] && exit 0

require_cmd chktex
# chktex exits 0 even with warnings; rely on non-empty stdout as the signal.
VIOLATIONS=$(chktex -q -n22 -n30 "$FILE_PATH" 2>&1 || true)
emit_post_tool_context "chktex" "$VIOLATIONS"
