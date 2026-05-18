#!/bin/bash
set -euCo pipefail
osascript -e 'display notification "Claude Code が許可を求めています" with title "Claude Code" subtitle "確認待ち"'
afplay /System/Library/Sounds/Submarine.aiff
