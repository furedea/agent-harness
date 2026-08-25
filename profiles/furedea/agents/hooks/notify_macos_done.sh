#!/bin/bash
set -euCo pipefail
osascript -e 'display notification "タスクが完了しました" with title "Claude Code" subtitle "処理終了"'
afplay /System/Library/Sounds/Blow.aiff
