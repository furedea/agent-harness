#!/bin/bash

# lint_format_txt.sh
# Quality Loop: autocorrect --fix (CJK spacing etc.). Format-only; emits no
# JSON. Always exits 0.

set -eo pipefail
# shellcheck source=lib/lint_format.sh
source "$(dirname "$0")/lib/lint_format.sh"

load_file_path # sets FILE_PATH, FILENAME

require_cmd autocorrect
autocorrect --fix "$FILE_PATH" >/dev/null 2>&1 || true
