#!/bin/bash
# Claude Code PreToolUse hook: regex-based allowlist for Bash commands needing precise control.
# Splits compound commands (|, ||, &&, ;, &) and validates each segment independently.
# Governed segments must match an allowed pattern; non-governed segments pass through.
# Exit code 0 = allow/pass-through, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/shell_parse.sh"
# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

# Require jq for JSON parsing.
if ! command -v jq >/dev/null 2>&1; then
  cat >&2 <<ERRMSG
BLOCKED: jq is not installed.

Why: This hook requires jq to parse tool input JSON. Without it, commands cannot be validated.

What to do:
  Claude Code: Ask the user to install jq.
  User: Install jq (e.g., brew install jq on macOS, sudo apt-get install jq on Linux).
ERRMSG
  exit 2
fi

INPUT=$(cat)
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
if ! RAW_COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null); then
  cat >&2 <<ERRMSG
BLOCKED: failed to parse tool input JSON.

Why: The hook received invalid JSON input and cannot validate the command.

What to do:
  Claude Code: Report this error to the user — it may indicate a Claude Code bug or misconfigured hook.
  User: Check that .claude/hooks/guard_allowed_commands.sh is correctly registered in settings.json.
ERRMSG
  exit 2
fi

# Normalize a command segment: trim whitespace and strip trailing shell redirections.
function normalize_segment() {
  echo "$1" | sed -E 's/^[[:space:]]+|[[:space:]]+$//; s/[[:space:]]+(2>&1|2>\/dev\/null|>&2)[[:space:]]*$//'
}

# Only govern specific command prefixes.
# Commands not listed here pass through to the built-in permission system.
GOVERNED_PREFIXES=(
  "actionlint"
  "autocorrect"
  "bats"
  "cargo"
  "commitlint"
  "deadnix"
  "dprint"
  "gh api"
  "gh issue"
  "gh label"
  "gh pr"
  "gh run"
  "git add"
  "git branch"
  "git commit"
  "git fetch"
  "git ls-files"
  "git pull"
  "git push"
  "git rebase"
  "git worktree"
  "nixfmt"
  "npm run"
  "npm test"
  "oxfmt"
  "oxlint"
  "pnpm"
  "prettierd"
  "selene"
  "shellcheck"
  "shfmt"
  "statix"
  "stylua"
  "tex-fmt"
  "tsgolint"
  "uv run"
)

# Allowed patterns (extended regex, matched against individual pipe segments)
# Add new patterns here to allow specific operations.
ALLOWED_PATTERNS=(
  # BATS tests — allow flags and any path under tests/
  '^bats( [^;&|<>$`]+)?$'

  # Local test/lint/format tools from nix/home/default.nix. These patterns
  # allow ordinary tool flags and paths while rejecting shell metacharacters.
  '^actionlint( [^;&|<>$`]+)?$'
  '^autocorrect( [^;&|<>$`]+)?$'
  '^cargo (test|fmt|clippy|check)( [^;&|<>$`]+)?$'
  '^commitlint( [^;&|<>$`]+)?$'
  '^deadnix( [^;&|<>$`]+)?$'
  '^dprint (check|fmt|output-file-paths)( [^;&|<>$`]+)?$'
  '^nixfmt( [^;&|<>$`]+)?$'
  '^npm test( [^;&|<>$`]+)?$'
  '^npm run (test|lint|format|typecheck|check|knip|knip:production)( [^;&|<>$`]+)?$'
  '^oxfmt( [^;&|<>$`]+)?$'
  '^oxlint( [^;&|<>$`]+)?$'
  '^pnpm (test|lint|format|typecheck|check)( [^;&|<>$`]+)?$'
  '^pnpm run (test|lint|format|format:check|typecheck|check|knip|knip:production)( [^;&|<>$`]+)?$'
  '^pnpm exec (vitest|tsc|oxfmt|oxlint|tsgolint|prettier|prettierd)( [^;&|<>$`]+)?$'
  '^prettierd( [^;&|<>$`]+)?$'
  '^selene( [^;&|<>$`]+)?$'
  '^shellcheck( [^;&|<>$`]+)?$'
  '^shfmt( [^;&|<>$`]+)?$'
  '^statix (check|fix)( [^;&|<>$`]+)?$'
  '^stylua( [^;&|<>$`]+)?$'
  '^tex-fmt( [^;&|<>$`]+)?$'
  '^tsgolint( [^;&|<>$`]+)?$'

  # Read PR comments (with optional --jq filter)
  '^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/comments( --paginate)?$'
  "^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/comments( --paginate)? --jq '[^']*'$"

  # Read issue comments (with optional --paginate and/or --jq filter)
  '^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/issues/[0-9]+/comments( --paginate)?$'
  "^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/issues/[0-9]+/comments( --paginate)? --jq '[^']*'$"

  # Reply to PR review comments (body must be single-quoted; apostrophes via '\'' escape)
  "^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/comments/[0-9]+/replies -f body='[^']*('\\\\''[^']*)*'$"

  # Read PR reviews and review comments (with optional --paginate and/or --jq filter)
  '^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/reviews( --paginate)?$'
  "^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/reviews( --paginate)? --jq '[^']*'$"
  '^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/reviews/[0-9]+/comments( --paginate)?$'
  "^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+/pulls/[0-9]+/reviews/[0-9]+/comments( --paginate)? --jq '[^']*'$"

  # Create sub-issues (used by /start-dev skill)
  '^gh api repos/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+ --method POST -f parent_issue_id=[a-zA-Z0-9_=]+ /issues/[0-9]+/sub_issues$'

  # GraphQL queries (read-only, any query allowed but must be single-quoted; apostrophes via '\'' escape)
  "^gh api graphql -f query='(query([[:space:]({{])|[{])[^']*('\\\\''[^']*)*'$"

  # GitHub issue operations — read-only (any flags allowed)
  '^gh issue (list|status|view)( |$)'
  # GitHub issue operations — write (any flags allowed)
  '^gh issue (create|comment|develop|edit|reopen)( |$)'

  # GitHub label operations (used by reporting skills to ensure labels exist)
  '^gh label (list|create)( |$)'

  # GitHub PR operations — read-only (any flags allowed)
  '^gh pr (list|status|checks|diff|view)( |$)'
  # GitHub PR operations — write (allowed with any args, review prompted commands individually)
  '^gh pr (create|checkout|comment|edit|ready)( |$)'

  # GitHub Actions run operations — read-only (any flags allowed)
  '^gh run (list|view|watch)( |$)'

  # Git add — stage files by path (no `.`, `-A`, or `--all`, which are also
  # blocked by permissions.deny in settings.json)
  '^git add [A-Za-z0-9._/-]+( [A-Za-z0-9._/-]+)*$'
  '^git add -- [A-Za-z0-9._/-]+( [A-Za-z0-9._/-]+)*$'

  # Git file listing — read-only file discovery for hooks and CI scripts.
  '^git ls-files "\*\.[A-Za-z0-9_-]+"$'

  # Git branch operations
  # Raw git branch
  '^git branch$'
  # Create a new branch: git branch <name>
  '^git branch [a-zA-Z0-9_./-]+$'
  # Delete a merged branch (safe -d only, not force -D): git branch -d <name>
  '^git branch -d [a-zA-Z0-9_./-]+$'
  # Show or list branches without changing state.
  '^git branch --show-current$'
  '^git branch --list( [a-zA-Z0-9_./-]+)?$'
  # Rename the current branch without force-overwriting an existing branch.
  '^git branch -m [a-zA-Z0-9_./-]+$'
  # List branches merged into a given branch: git branch --merged <name>
  '^git branch --merged [a-zA-Z0-9_./-]+$'

  # Git commit
  "^git commit -m '[^']*('\\\\''[^']*)*'$"
  '^git commit -m "[^"$`\\]*"$'
  '^git commit --amend -m [^;&|<>$`]+$'

  # Git pull
  '^git pull( --rebase)?( origin [a-zA-Z0-9_./-]+)?$'
  '^git pull --ff-only$'

  # Git fetch / rebase for pre-PR branch refresh. Keep rebase narrow: only
  # replay the current branch onto an origin/* base, or continue/abort an
  # already-started rebase.
  '^git fetch origin$'
  '^git rebase origin/[a-zA-Z0-9_./-]+$'
  '^git rebase --(continue|abort)$'

  # Git push
  '^git push$'
  '^git push origin$'
  '^git push (-u |--set-upstream )?origin [a-zA-Z0-9_./-]+$'

  # Git worktree operations for isolated task branches. Maintenance and
  # deletion commands intentionally stay out of the allowlist.
  '^git worktree list$'
  '^git worktree add -b [a-zA-Z0-9_./-]+ [A-Za-z0-9._/-]+ origin/[a-zA-Z0-9_./-]+$'

  # Python development commands. Require --frozen so agents do not implicitly
  # resolve or update dependencies while running quality gates.
  '^uv run --frozen ruff( check| format --check)?$'
  '^uv run --frozen --group audit (deptry \.|vulture)$'
  '^uv run --frozen ruff (check|format)( --(fix|quiet|check))*( [^;&|<>$`]+)?$'
  '^uv run --frozen ty check( [^;&|<>$`]+)?$'
  '^uv run --frozen pytest( [^;&|<>$`]+)?$'
  '^uv run --frozen --with pytest pytest( [^;&|<>$`]+)?$'
  '^uv run --frozen python scripts/(collect_transcripts|collect_skills|generate_report|apply_patches|run_audit)\.py( [^;&|<>$`]+)?$'

)

# Denied patterns for commands that belong to a governed family but must never
# be accepted by the broad positive regexes below.
DENIED_PATTERNS=(
  '(^|[[:space:]])([^[:space:]]*/)?\.venv/bin/python([0-9.]*)?([[:space:]]|$)'
  '^git[[:space:]]+add[[:space:]]+(\.|-A|--all)([[:space:]]|$)'
  '(^|[[:space:]])git[[:space:]]+add[[:space:]]+(\.|-A|--all)([[:space:]]|$)'
  '^git[[:space:]]+commit([[:space:]].*)?[[:space:]]+(--no-verify|-n)([[:space:]]|$)'
  '(^|[[:space:]])git[[:space:]]+commit([[:space:]].*)?[[:space:]]+(--no-verify|-n)([[:space:]]|$)'
)

# Validate each segment of the pipeline independently.
# Governed segments must match an allowed pattern; non-governed segments pass through.
BLOCKED_SEGMENT=""
BLOCKED_REASON=""
while IFS= read -r segment; do
  segment=$(normalize_segment "$segment")
  [ -z "$segment" ] && continue

  # Explicitly block known-bad forms before applying the positive allowlist.
  for pattern in "${DENIED_PATTERNS[@]}"; do
    if echo "$segment" | grep -qE -e "$pattern"; then
      BLOCKED_SEGMENT="$segment"
      BLOCKED_REASON="command denied by allowlist policy"
      break
    fi
  done
  [ -n "$BLOCKED_SEGMENT" ] && break

  # Check if this segment is governed
  segment_governed=false
  for prefix in "${GOVERNED_PREFIXES[@]}"; do
    if [[ "$segment" == "$prefix"* ]]; then
      segment_governed=true
      break
    fi
  done

  [ "$segment_governed" = false ] && continue # Not governed — pass through

  # Governed segment: must match an allowed pattern
  segment_allowed=false
  for pattern in "${ALLOWED_PATTERNS[@]}"; do
    if echo "$segment" | grep -qE -e "$pattern"; then
      segment_allowed=true
      break
    fi
  done

  if [ "$segment_allowed" = false ]; then
    BLOCKED_SEGMENT="$segment"
    BLOCKED_REASON="command not in allowlist"
    break
  fi
done <<<"$(split_command_segments "$RAW_COMMAND")"

# If all segments passed, allow the command
if [ -z "$BLOCKED_SEGMENT" ]; then
  exit 0
fi

# A governed segment was not in the allowlist — block the entire command
log_blocked Bash "$RAW_COMMAND" "$BLOCKED_REASON: $BLOCKED_SEGMENT" guard_allowed_commands.sh "$SESSION"
cat >&2 <<ERRMSG
BLOCKED: $BLOCKED_REASON.

Command: $BLOCKED_SEGMENT

Why:
  This command segment (for example, this gh api endpoint/flag combination) is not on the approved allowlist,
  or it matches an explicitly denied form such as bulk git add.

What to do:
  Claude Code: Try a different approach, or ask the user whether this command should be allowed.
  User: To allow this command, add a regex pattern to .claude/hooks/guard_allowed_commands.sh
        or run the command manually in your terminal.
ERRMSG

exit 2
