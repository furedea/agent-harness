#!/bin/bash
set -euCo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  exit 0
fi

command -v osascript >/dev/null 2>&1 || exit 0
command -v afplay >/dev/null 2>&1 || exit 0

osascript -e 'display notification "タスクが完了しました" with title "Claude Code" subtitle "処理終了"'
afplay /System/Library/Sounds/Blow.aiff
