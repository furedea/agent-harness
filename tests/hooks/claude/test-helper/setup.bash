# Shared setup for hook tests.
# Source this file from each .bats file's setup() function.

# Absolute path to the repository root (three levels up from tests/hooks/claude/).
REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"

# Path to hook scripts under test.
HOOK_DIR="$REPO_ROOT/agents/hooks"

# Build a JSON payload matching the PreToolUse hook input format.
# Uses jq for proper escaping of special characters.
# Usage: make_input "git commit -m 'hello'"
make_input() {
  jq -n --arg cmd "$1" '{"tool_input":{"command":$cmd}}'
}

# Build a JSON payload matching the PostToolUse hook input format for Edit/Write tools.
# Usage: make_post_tool_input "/path/to/file.py"
make_post_tool_input() {
  jq -n --arg fp "$1" '{"tool_input":{"file_path":$fp},"tool_response":{"success":true}}'
}

# Create a temporary git repository for tests that need staged files.
# Sets TEMP_REPO to the path. Caller should cd into it.
create_temp_git_repo() {
  TEMP_REPO="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/repo.XXXXXX")"
  git -C "$TEMP_REPO" init --quiet
  git -C "$TEMP_REPO" config user.email "test@test.com"
  git -C "$TEMP_REPO" config user.name "Test"
  git -C "$TEMP_REPO" config commit.gpgsign false
  git -C "$TEMP_REPO" config core.fsmonitor false
  # Create an initial commit so HEAD exists
  touch "$TEMP_REPO/.gitkeep"
  git -C "$TEMP_REPO" add .gitkeep
  git -C "$TEMP_REPO" commit --quiet -m "initial"
}

# Stage a file with given content in the temp repo.
# Usage: stage_file "path/to/file" "file contents"
stage_file() {
  local filepath="$1"
  local content="${2:-}"
  local dir
  dir="$(dirname "$TEMP_REPO/$filepath")"
  mkdir -p "$dir"
  printf '%s\n' "$content" > "$TEMP_REPO/$filepath"
  git -C "$TEMP_REPO" add "$filepath"
}

# Build a JSON payload for guard_secret_content.sh prompt mode.
# Usage: make_prompt_input "some user prompt text"
make_prompt_input() {
  jq -n --arg p "$1" '{"prompt":$p}'
}

# Build a JSON payload for guard_secret_content.sh read mode.
# Usage: make_read_input "/path/to/file"
make_read_input() {
  jq -n --arg fp "$1" '{"tool_input":{"file_path":$fp}}'
}

# Build a JSON payload for guard_secret_content.sh write mode.
# Usage: make_write_input "content" "new_string"
make_write_input() {
  jq -n --arg c "${1:-}" --arg ns "${2:-}" '{"tool_input":{"content":$c,"new_string":$ns}}'
}

# Build a JSON payload for audit_tool_call.sh / audit_permission_denied.sh.
# Usage: make_log_input "Bash" '{"command":"ls"}' "session-123" "PreToolUse"
make_log_input() {
  local tool="$1"
  local tool_input="$2"
  local session="${3:-test-session}"
  local event="${4:-PostToolUse}"
  jq -n --arg t "$tool" --argjson ti "$tool_input" --arg s "$session" --arg e "$event" \
    '{"hook_event_name":$e,"tool_name":$t,"tool_input":$ti,"session_id":$s}'
}

# Build a JSON payload for audit_compaction.sh (PreCompact event).
# Usage: make_compact_input "manual" "session-123" "/path/to/transcript.jsonl"
make_compact_input() {
  local trigger="${1:-manual}"
  local session="${2:-test-session}"
  local transcript="${3:-}"
  jq -n --arg trig "$trigger" --arg s "$session" --arg tp "$transcript" \
    '{"hook_event_name":"PreCompact","trigger":$trig,"session_id":$s,"transcript_path":$tp}'
}

# Build a JSON payload for the SessionStart event (post-compaction probe).
# Usage: make_session_start_input "compact" "session-123" "/path/to/transcript.jsonl"
make_session_start_input() {
  local source="${1:-startup}"
  local session="${2:-test-session}"
  local transcript="${3:-}"
  jq -n --arg src "$source" --arg s "$session" --arg tp "$transcript" \
    '{"hook_event_name":"SessionStart","source":$src,"session_id":$s,"transcript_path":$tp}'
}

# Build a JSON payload for audit_permission_denied.sh (includes reason).
# Usage: make_denied_input "Bash" '{"command":"rm -rf /"}' "auto-mode denied" "session-123"
make_denied_input() {
  local tool="$1"
  local tool_input="$2"
  local reason="${3:-denied}"
  local session="${4:-test-session}"
  jq -n --arg t "$tool" --argjson ti "$tool_input" --arg r "$reason" --arg s "$session" \
    '{"tool_name":$t,"tool_input":$ti,"reason":$r,"session_id":$s}'
}
