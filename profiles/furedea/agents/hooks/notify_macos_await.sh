#!/usr/bin/env bash
set -euCo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  exit 0
fi

command -v osascript >/dev/null 2>&1 || exit 0
command -v afplay >/dev/null 2>&1 || exit 0

osascript -e 'display notification "Claude Code が許可を求めています" with title "Claude Code" subtitle "確認待ち"'
afplay /System/Library/Sounds/Submarine.aiff
