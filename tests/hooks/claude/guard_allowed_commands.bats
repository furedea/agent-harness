#!/usr/bin/env bats
# Tests for .claude/hooks/guard_allowed_commands.sh
#
# Focus areas:
# 1. Compound command splitting (|, ||, &&, ;, &)
# 2. Redirection handling (2>&1, >&2, 2>/dev/null and & disambiguation)
# 3. Segment normalization (whitespace trimming, redirection stripping)
# 4. Governed vs non-governed boundary (pass-through logic)
# 5. Quote-aware splitting (single quotes protect operators)
#
# Individual allowlist patterns are NOT exhaustively tested here —
# they are project-specific and may change. The tests below verify
# the mechanism, not the policy.

setup() {
  load test-helper/setup
}

run_hook() {
  run bash "$HOOK_DIR/guard_allowed_commands.sh" <<<"$(make_input "$1")"
}

# ============================================================
# Governed vs non-governed boundary
# ============================================================

@test "passes through non-governed commands unchanged" {
  run_hook "echo hello"
  [ "$status" -eq 0 ]
}

@test "passes through git commands that are not governed" {
  run_hook "git status"
  [ "$status" -eq 0 ]
}

@test "passes through empty command" {
  run bash "$HOOK_DIR/guard_allowed_commands.sh" <<<'{"tool_input":{"command":""}}'
  [ "$status" -eq 0 ]
}

@test "governed command matching allowlist is permitted" {
  # gh pr list is a universally expected allowlist entry
  run_hook "gh pr list"
  [ "$status" -eq 0 ]
}

@test "allows Python TDD commands" {
  run_hook "uv run --frozen ruff check"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ruff format --check"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ty check"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ty check src tests"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen pytest"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen pytest tests/test_main.py -k test_main --cov"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen pytest tests/test_main.py"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen --with pytest pytest tests/test_main.py -k test_main"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen python scripts/run_audit.py prepare --provider codex --days 14"
  [ "$status" -eq 0 ]
}

@test "allows Python and TypeScript audit commands" {
  run_hook "uv run --frozen --group audit deptry ."
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen --group audit vulture"
  [ "$status" -eq 0 ]

  run_hook "pnpm run knip"
  [ "$status" -eq 0 ]

  run_hook "pnpm run knip:production"
  [ "$status" -eq 0 ]
}

@test "allows git commit messages with single or double quotes" {
  run_hook "git commit -m 'feat(test): allow single quoted messages'"
  [ "$status" -eq 0 ]

  run_hook 'git commit -m "feat(test): allow double quoted messages"'
  [ "$status" -eq 0 ]
}

@test "allows Python style frozen ruff commands" {
  run_hook "uv run --frozen ruff check"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ruff check src/main.py"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ruff format --check"
  [ "$status" -eq 0 ]

  run_hook "uv run --frozen ruff format tests/test_main.py"
  [ "$status" -eq 0 ]
}

@test "allows local test lint and format tools from home packages" {
  run_hook "bats tests/hooks/claude/guard_allowed_commands.bats"
  [ "$status" -eq 0 ]

  run_hook "actionlint .github/workflows/ci.yml"
  [ "$status" -eq 0 ]

  run_hook "shellcheck agents/hooks/guard_allowed_commands.sh"
  [ "$status" -eq 0 ]

  run_hook "shfmt -w agents/hooks/guard_allowed_commands.sh"
  [ "$status" -eq 0 ]

  run_hook "dprint check"
  [ "$status" -eq 0 ]

  run_hook "dprint fmt README.md"
  [ "$status" -eq 0 ]

  run_hook "nixfmt nix/home/default.nix"
  [ "$status" -eq 0 ]

  run_hook "statix check nix"
  [ "$status" -eq 0 ]

  run_hook "deadnix nix"
  [ "$status" -eq 0 ]
}

@test "allows Rust TypeScript Lua and LaTeX quality commands" {
  run_hook "cargo test"
  [ "$status" -eq 0 ]

  run_hook "cargo clippy --all-targets --all-features"
  [ "$status" -eq 0 ]

  run_hook "cargo fmt --check"
  [ "$status" -eq 0 ]

  run_hook "pnpm test -- --run"
  [ "$status" -eq 0 ]

  run_hook "pnpm exec oxlint src"
  [ "$status" -eq 0 ]

  run_hook "npm run lint"
  [ "$status" -eq 0 ]

  run_hook "oxfmt --check src"
  [ "$status" -eq 0 ]

  run_hook "oxlint src"
  [ "$status" -eq 0 ]

  run_hook "tsgolint --project tsconfig.json"
  [ "$status" -eq 0 ]

  run_hook "stylua --check nvim"
  [ "$status" -eq 0 ]

  run_hook "selene nvim"
  [ "$status" -eq 0 ]

  run_hook "tex-fmt --check docs/main.tex"
  [ "$status" -eq 0 ]
}

@test "blocks shell metacharacters in local quality commands" {
  run_hook "cargo test > /tmp/blocked"
  [ "$status" -eq 2 ]

  run_hook 'pnpm test $(touch /tmp/blocked)'
  [ "$status" -eq 2 ]

  run_hook 'dprint fmt README.md $(touch /tmp/blocked)'
  [ "$status" -eq 2 ]
}

@test "blocks shell metacharacters in broad pytest command" {
  run_hook 'uv run --frozen pytest $(touch /tmp/blocked)'
  [ "$status" -eq 2 ]

  run_hook 'uv run --frozen pytest `touch /tmp/blocked`'
  [ "$status" -eq 2 ]

  run_hook "uv run --frozen pytest > /tmp/blocked"
  [ "$status" -eq 2 ]

  run_hook 'uv run --frozen --with pytest pytest $(touch /tmp/blocked)'
  [ "$status" -eq 2 ]

  run_hook 'uv run --with ruff ruff check'
  [ "$status" -eq 2 ]

  run_hook 'uv run python -c "print(1)"'
  [ "$status" -eq 2 ]
}

@test "blocks uv run commands without frozen" {
  run_hook "uv run ruff check"
  [ "$status" -eq 2 ]

  run_hook "uv run ty check"
  [ "$status" -eq 2 ]

  run_hook "uv run pytest"
  [ "$status" -eq 2 ]

  run_hook "uv run --with pytest pytest"
  [ "$status" -eq 2 ]

  run_hook "uv run --group audit deptry ."
  [ "$status" -eq 2 ]

  run_hook "uv run python scripts/run_audit.py prepare"
  [ "$status" -eq 2 ]
}

@test "blocks virtualenv Python interpreter bypasses" {
  run_hook ".venv/bin/python -c 'print(1)'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"denied by allowlist policy"* ]]

  run_hook "./.venv/bin/python3 scripts/run_audit.py"
  [ "$status" -eq 2 ]

  run_hook "/tmp/project/.venv/bin/python -m pytest"
  [ "$status" -eq 2 ]
}

@test "blocks command substitution in double quoted git commit messages" {
  run_hook 'git commit -m "feat(test): $(touch /tmp/blocked)"'
  [ "$status" -eq 2 ]

  run_hook 'git commit -m "feat(test): `touch /tmp/blocked`"'
  [ "$status" -eq 2 ]
}

@test "allows git add explicit paths" {
  run_hook "git add bot/main.py"
  [ "$status" -eq 0 ]

  run_hook "git add bot/main.py tests/bot/test_main.py"
  [ "$status" -eq 0 ]

  run_hook "git add -- path/to/file"
  [ "$status" -eq 0 ]
}

@test "allows git ls-files extension globs" {
  run_hook 'git ls-files "*.nix"'
  [ "$status" -eq 0 ]

  run_hook 'git ls-files "*.ts"'
  [ "$status" -eq 0 ]
}

@test "blocks broad git ls-files forms" {
  run_hook "git ls-files"
  [ "$status" -eq 2 ]

  run_hook 'git ls-files "."'
  [ "$status" -eq 2 ]

  run_hook 'git ls-files "*.nix" --others'
  [ "$status" -eq 2 ]
}

@test "allows safe git branch helper operations" {
  run_hook "git branch --show-current"
  [ "$status" -eq 0 ]

  run_hook "git branch --list feat/topic"
  [ "$status" -eq 0 ]

  run_hook "git branch -m feat/topic"
  [ "$status" -eq 0 ]
}

@test "allows worktree branch creation commands" {
  run_hook "git worktree list"
  [ "$status" -eq 0 ]

  run_hook "git worktree add -b feat/worktree-branch-delivery ../agent-harness-feat-worktree-branch-delivery origin/main"
  [ "$status" -eq 0 ]
}

@test "blocks worktree maintenance commands" {
  run_hook "git worktree remove ../agent-harness-feat-worktree-branch-delivery"
  [ "$status" -eq 2 ]

  run_hook "git worktree prune"
  [ "$status" -eq 2 ]

  run_hook "git worktree move ../old ../new"
  [ "$status" -eq 2 ]

  run_hook "git worktree repair"
  [ "$status" -eq 2 ]
}

@test "allows PR delivery commands" {
  run_hook "git fetch origin"
  [ "$status" -eq 0 ]

  run_hook "git pull --ff-only"
  [ "$status" -eq 0 ]

  run_hook "git push -u origin feat/agent-pr-delivery"
  [ "$status" -eq 0 ]

  run_hook "git push --set-upstream origin fix/parser-empty-input"
  [ "$status" -eq 0 ]

  run_hook "gh pr create -f --base main"
  [ "$status" -eq 0 ]
}

@test "allows narrow pre-PR rebase commands" {
  run_hook "git rebase origin/main"
  [ "$status" -eq 0 ]

  run_hook "git rebase origin/release/1.2"
  [ "$status" -eq 0 ]

  run_hook "git rebase --continue"
  [ "$status" -eq 0 ]

  run_hook "git rebase --abort"
  [ "$status" -eq 0 ]
}

@test "blocks broad rebase commands" {
  run_hook "git rebase -i origin/main"
  [ "$status" -eq 2 ]

  run_hook "git rebase --onto origin/main HEAD~2"
  [ "$status" -eq 2 ]

  run_hook "git rebase main"
  [ "$status" -eq 2 ]
}

@test "blocks bulk git add forms" {
  run_hook "git add ."
  [ "$status" -eq 2 ]
  [[ "$output" == *"denied by allowlist policy"* ]]

  run_hook "git add -A"
  [ "$status" -eq 2 ]

  run_hook "git add --all"
  [ "$status" -eq 2 ]

  run_hook "ls && git add ."
  [ "$status" -eq 2 ]

  run_hook "echo ok | xargs -I {} git add -A"
  [ "$status" -eq 2 ]

  run_hook "git   add   ."
  [ "$status" -eq 2 ]

  run_hook "git add --all --verbose"
  [ "$status" -eq 2 ]
}

@test "allows normal git commit forms" {
  run_hook 'git commit -m "hello world"'
  [ "$status" -eq 0 ]

  run_hook "git commit --amend -m fix"
  [ "$status" -eq 0 ]
}

@test "blocks git commit no-verify bypass forms" {
  run_hook "git commit --no-verify -m test"
  [ "$status" -eq 2 ]
  [[ "$output" == *"denied by allowlist policy"* ]]

  run_hook "git commit -m test --no-verify"
  [ "$status" -eq 2 ]

  run_hook "git commit -n -m test"
  [ "$status" -eq 2 ]

  run_hook "git   commit   --no-verify"
  [ "$status" -eq 2 ]
}

@test "governed command not in allowlist is blocked" {
  run_hook "gh api repos/owner/repo/unknown-endpoint"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

@test "blocked output includes the offending command segment" {
  run_hook "gh pr merge 42"
  [ "$status" -eq 2 ]
  [[ "$output" == *"gh pr merge 42"* ]]
}

# ============================================================
# Compound command splitting — pipe |
# ============================================================

@test "splits on pipe: governed | non-governed" {
  run_hook "gh pr list | head -5"
  [ "$status" -eq 0 ]
}

@test "splits on pipe: non-governed | governed" {
  run_hook "echo test | gh pr list"
  [ "$status" -eq 0 ]
}

@test "splits on pipe: blocked segment in pipeline" {
  run_hook "gh pr list | gh pr merge 42"
  [ "$status" -eq 2 ]
}

# ============================================================
# Compound command splitting — logical AND &&
# ============================================================

@test "splits on &&: governed && non-governed" {
  run_hook "gh pr list && echo done"
  [ "$status" -eq 0 ]
}

@test "splits on &&: blocked segment after &&" {
  run_hook "gh pr list && gh pr merge 42"
  [ "$status" -eq 2 ]
}

@test "splits on &&: blocked segment before &&" {
  run_hook "gh pr merge 42 && gh pr list"
  [ "$status" -eq 2 ]
}

# ============================================================
# Compound command splitting — logical OR ||
# ============================================================

@test "splits on ||: governed || non-governed" {
  run_hook "gh pr list || echo failed"
  [ "$status" -eq 0 ]
}

@test "splits on ||: blocked segment after ||" {
  run_hook "gh pr list || gh pr merge 42"
  [ "$status" -eq 2 ]
}

# ============================================================
# Compound command splitting — semicolon ;
# ============================================================

@test "splits on semicolon: governed; non-governed" {
  run_hook "gh pr list; echo done"
  [ "$status" -eq 0 ]
}

@test "splits on semicolon: blocked segment after ;" {
  run_hook "gh pr list; gh pr merge 42"
  [ "$status" -eq 2 ]
}

# ============================================================
# Compound command splitting — background &
# ============================================================

@test "splits on background &" {
  run_hook "gh pr list & echo done"
  [ "$status" -eq 0 ]
}

@test "splits on background &: blocked segment" {
  run_hook "gh pr list & gh pr merge 42"
  [ "$status" -eq 2 ]
}

# ============================================================
# & disambiguation: redirections vs background operator
# ============================================================

@test "does NOT split on & inside 2>&1 redirection" {
  run_hook "gh pr list 2>&1"
  [ "$status" -eq 0 ]
}

@test "does NOT split on & inside >&2 redirection" {
  run_hook "gh pr list >&2"
  [ "$status" -eq 0 ]
}

@test "does NOT split on & inside 2>/dev/null" {
  # 2>/dev/null doesn't contain &, but verify it doesn't confuse the parser
  run_hook "gh pr list 2>/dev/null"
  [ "$status" -eq 0 ]
}

@test "background & after redirection is still split" {
  run_hook "gh pr list 2>&1 & echo done"
  [ "$status" -eq 0 ]
}

# ============================================================
# Normalization — whitespace trimming
# ============================================================

@test "trims leading whitespace from segments" {
  run_hook "  gh pr list"
  [ "$status" -eq 0 ]
}

@test "trims trailing whitespace from segments" {
  run_hook "gh pr list  "
  [ "$status" -eq 0 ]
}

@test "trims whitespace from both ends" {
  run_hook "  gh pr list  "
  [ "$status" -eq 0 ]
}

@test "trims whitespace in piped segments" {
  run_hook "  gh pr list  |  head -5  "
  [ "$status" -eq 0 ]
}

# ============================================================
# Normalization — trailing redirection stripping
# ============================================================

@test "strips trailing 2>&1 before matching" {
  run_hook "gh pr list 2>&1"
  [ "$status" -eq 0 ]
}

@test "strips trailing >&2 before matching" {
  run_hook "gh pr list >&2"
  [ "$status" -eq 0 ]
}

@test "strips trailing 2>/dev/null before matching" {
  run_hook "gh pr list 2>/dev/null"
  [ "$status" -eq 0 ]
}

# ============================================================
# Quote-aware splitting — single quotes protect operators
# ============================================================

@test "does not split on pipe inside single quotes" {
  run_hook "gh api repos/owner/repo/pulls/1/comments --jq '.[].body | length'"
  [ "$status" -eq 0 ]
}

@test "does not split on semicolon inside single quotes" {
  run_hook "gh api repos/owner/repo/pulls/1/comments --jq '.[] ; .body'"
  [ "$status" -eq 0 ]
}

@test "does not split on && inside single quotes" {
  run_hook "gh api repos/owner/repo/pulls/1/comments --jq '.[] && .body'"
  [ "$status" -eq 0 ]
}

@test "does not split on || inside single quotes" {
  run_hook "gh api repos/owner/repo/pulls/1/comments --jq '.[] || .body'"
  [ "$status" -eq 0 ]
}

@test "handles apostrophe escape in single-quoted body" {
  # Shell: -f body='it'\''s great'  →  the '\'' sequence ends quote, adds literal ', reopens quote
  local input
  input=$(jq -n --arg cmd "gh api repos/owner/repo/pulls/1/comments/99/replies -f body='it'\\''s great'" '{tool_input:{command:$cmd}}')
  run bash "$HOOK_DIR/guard_allowed_commands.sh" <<<"$input"
  [ "$status" -eq 0 ]
}

# ============================================================
# Multiple governed segments
# ============================================================

@test "allows when all governed segments match allowlist" {
  run_hook "gh pr list && gh pr status"
  [ "$status" -eq 0 ]
}

@test "blocks when any governed segment is not allowed" {
  run_hook "gh pr list && gh pr merge 42 && gh pr status"
  [ "$status" -eq 2 ]
}

# ============================================================
# Error handling
# ============================================================

@test "handles invalid JSON gracefully" {
  run bash "$HOOK_DIR/guard_allowed_commands.sh" <<<"not json"
  [ "$status" -eq 2 ]
  [[ "$output" == *"failed to parse"* ]]
}
