#!/bin/bash

# lint_format_md.sh
# Quality Loop: autocorrect --fix -> prettierd (atomic swap via stdin/stdout).
# Format-only; emits no JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd autocorrect
autocorrect --fix "$FILE_PATH" >/dev/null 2>&1 || true

require_cmd prettierd
TMPFILE=$(mktemp)
# shellcheck disable=SC2094
if PRETTIERD_DEFAULT_CONFIG="$HOME/.prettierrc" \
  prettierd "$FILE_PATH" <"$FILE_PATH" >"$TMPFILE" 2>/dev/null; then
  mv "$TMPFILE" "$FILE_PATH"
else
  rm -f "$TMPFILE"
fi
