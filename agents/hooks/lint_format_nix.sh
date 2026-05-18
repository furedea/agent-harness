#!/bin/bash

# lint_format_nix.sh
# Quality Loop: nixfmt -> statix --fix -> capture residual statix + deadnix
# diagnostics via PostToolUse additionalContext JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd nixfmt
nixfmt "$FILE_PATH" >/dev/null 2>&1 || true

require_cmd statix
statix fix "$FILE_PATH" >/dev/null 2>&1 || true
STATIX_OUT=""
if ! STATIX_OUT=$(statix check "$FILE_PATH" 2>&1); then
  emit_post_tool_context "statix" "$STATIX_OUT"
fi

require_cmd deadnix
DEADNIX_OUT=""
if ! DEADNIX_OUT=$(deadnix --fail "$FILE_PATH" 2>&1); then
  emit_post_tool_context "deadnix" "$DEADNIX_OUT"
fi
