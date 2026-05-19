#!/usr/bin/env bats
# Tests for .claude/hooks/guard_dangerous_git.sh

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/guard_dangerous_git.sh"
}

# --- Allowed: normal pushes ---

@test "allows push to feature branch" {
  run bash "$HOOK" <<< "$(make_input 'git push origin feature/foo')"
  [ "$status" -eq 0 ]
}

@test "allows -u origin feature/foo (boolean -u, does not consume origin)" {
  run bash "$HOOK" <<< "$(make_input 'git push -u origin feature/foo')"
  [ "$status" -eq 0 ]
}

@test "allows --set-upstream origin feature/bar (boolean --set-upstream)" {
  run bash "$HOOK" <<< "$(make_input 'git push --set-upstream origin feature/bar')"
  [ "$status" -eq 0 ]
}

@test "allows bare git push from feature branch" {
  create_temp_git_repo
  git -C "$TEMP_REPO" checkout -b feature/bar --quiet
  (
    cd "$TEMP_REPO"
    run bash "$HOOK" <<< "$(make_input 'git push')"
    [ "$status" -eq 0 ]
  )
}

@test "allows git pull (not governed)" {
  run bash "$HOOK" <<< "$(make_input 'git pull origin main')"
  [ "$status" -eq 0 ]
}

# --- Allowed: merges and passthrough ---

@test "allows gh pr merge without --admin" {
  run bash "$HOOK" <<< "$(make_input 'gh pr merge 42 --squash')"
  [ "$status" -eq 0 ]
}

@test "allows gh pr merge with rebase" {
  run bash "$HOOK" <<< "$(make_input 'gh pr merge 42 --rebase')"
  [ "$status" -eq 0 ]
}

@test "passes through ls" {
  run bash "$HOOK" <<< "$(make_input 'ls -la')"
  [ "$status" -eq 0 ]
}

@test "passes through git status" {
  run bash "$HOOK" <<< "$(make_input 'git status')"
  [ "$status" -eq 0 ]
}

@test "passes through empty command" {
  run bash "$HOOK" <<< '{"tool_input":{"command":""}}'
  [ "$status" -eq 0 ]
}

# --- Blocked: force push flags ---

@test "blocks --force" {
  run bash "$HOOK" <<< "$(make_input 'git push --force origin feature/foo')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"force-push flag"* ]]
}

@test "blocks -f short flag" {
  run bash "$HOOK" <<< "$(make_input 'git push -f origin feature/foo')"
  [ "$status" -eq 2 ]
}

@test "blocks --force-with-lease" {
  run bash "$HOOK" <<< "$(make_input 'git push --force-with-lease origin feature/foo')"
  [ "$status" -eq 2 ]
}

@test "blocks --force-with-lease with value" {
  run bash "$HOOK" <<< "$(make_input 'git push --force-with-lease=origin/foo origin feature/foo')"
  [ "$status" -eq 2 ]
}

# --- Blocked: +refspec force ---

@test "blocks +refspec" {
  run bash "$HOOK" <<< "$(make_input 'git push origin +feature/foo')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"refspec"* ]]
}

@test "blocks +src:dst refspec" {
  run bash "$HOOK" <<< "$(make_input 'git push origin +feature/foo:main')"
  [ "$status" -eq 2 ]
}

# --- Blocked: explicit push to main/master ---

@test "blocks explicit origin main" {
  run bash "$HOOK" <<< "$(make_input 'git push origin main')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"main"* ]]
}

@test "blocks explicit origin master" {
  run bash "$HOOK" <<< "$(make_input 'git push origin master')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"master"* ]]
}

@test "blocks HEAD:main refspec" {
  run bash "$HOOK" <<< "$(make_input 'git push origin HEAD:main')"
  [ "$status" -eq 2 ]
}

@test "blocks src:main refspec" {
  run bash "$HOOK" <<< "$(make_input 'git push origin feature/foo:main')"
  [ "$status" -eq 2 ]
}

@test "blocks multi-refspec with main included" {
  run bash "$HOOK" <<< "$(make_input 'git push -u origin feature/foo main')"
  [ "$status" -eq 2 ]
}

# --- Blocked: implicit push when current branch is main ---

@test "blocks implicit push from main" {
  create_temp_git_repo
  # Normalize the initial branch to 'main' regardless of git default.
  git -C "$TEMP_REPO" branch -M main
  (
    cd "$TEMP_REPO"
    run bash "$HOOK" <<< "$(make_input 'git push')"
    [ "$status" -eq 2 ]
    [[ "$output" == *"main"* ]]
  )
}

@test "allows implicit push from feature branch" {
  create_temp_git_repo
  git -C "$TEMP_REPO" checkout -b feature/foo --quiet
  (
    cd "$TEMP_REPO"
    run bash "$HOOK" <<< "$(make_input 'git push')"
    [ "$status" -eq 0 ]
  )
}

# --- Blocked: gh pr merge --admin ---

@test "blocks gh pr merge --admin (trailing)" {
  run bash "$HOOK" <<< "$(make_input 'gh pr merge 42 --admin')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"admin"* ]]
}

@test "blocks gh pr merge with --admin in middle" {
  run bash "$HOOK" <<< "$(make_input 'gh pr merge --admin 42')"
  [ "$status" -eq 2 ]
}

# --- Blocked: compound-command bypass ---

@test "blocks push to main after cd (&&)" {
  run bash "$HOOK" <<< "$(make_input 'cd /tmp && git push origin main')"
  [ "$status" -eq 2 ]
}

@test "blocks --force in piped segment" {
  run bash "$HOOK" <<< "$(make_input 'foo | git push --force origin feature/foo')"
  [ "$status" -eq 2 ]
}

@test "blocks push to main after semicolon" {
  run bash "$HOOK" <<< "$(make_input 'git status; git push origin main')"
  [ "$status" -eq 2 ]
}

@test "allows pipe with no destructive segment" {
  run bash "$HOOK" <<< "$(make_input 'ls | cat')"
  [ "$status" -eq 0 ]
}

# --- Blocked: shell wrappers carrying destructive tokens ---

@test "blocks bash -c with --force" {
  run bash "$HOOK" <<< "$(make_input 'bash -c "git push --force origin main"')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"wrapper"* ]]
}

@test "blocks sh -c with main push" {
  run bash "$HOOK" <<< "$(make_input 'sh -c "git push origin main"')"
  [ "$status" -eq 2 ]
}

@test "blocks /bin/bash -c with --force" {
  run bash "$HOOK" <<< "$(make_input '/bin/bash -c "git push --force"')"
  [ "$status" -eq 2 ]
}

@test "blocks eval with --force" {
  run bash "$HOOK" <<< "$(make_input 'eval "git push --force origin main"')"
  [ "$status" -eq 2 ]
}

@test "blocks zsh -c with admin merge" {
  run bash "$HOOK" <<< "$(make_input 'zsh -c "gh pr merge 42 --admin"')"
  [ "$status" -eq 2 ]
}

# --- Allowed: shell wrappers with benign content ---

@test "allows bash -c with ls" {
  run bash "$HOOK" <<< "$(make_input 'bash -c "ls -la"')"
  [ "$status" -eq 0 ]
}

@test "allows bash -c with echo" {
  run bash "$HOOK" <<< '{"tool_input":{"command":"bash -c \"echo hello\""}}'
  [ "$status" -eq 0 ]
}

# --- Blocked messages carry segment context ---

@test "blocked force-push message includes the offending segment" {
  run bash "$HOOK" <<< "$(make_input 'git push --force origin feature/foo')"
  [ "$status" -eq 2 ]
  [[ "$output" == *"Segment:"* ]]
  [[ "$output" == *"git push --force origin feature/foo"* ]]
}

# --- Blocked: destructive verbs (always-destructive, regardless of args) ---

@test "blocks git rm" {
  run bash "$HOOK" <<< "$(make_input 'git rm foo.py')"
  [ "$status" -eq 2 ]
}

@test "blocks git rm --cached" {
  run bash "$HOOK" <<< "$(make_input 'git rm --cached foo.py')"
  [ "$status" -eq 2 ]
}

@test "blocks git rm -r" {
  run bash "$HOOK" <<< "$(make_input 'git rm -r dir/')"
  [ "$status" -eq 2 ]
}

@test "blocks git clean -fd" {
  run bash "$HOOK" <<< "$(make_input 'git clean -fd')"
  [ "$status" -eq 2 ]
}

@test "blocks git clean -n (dry-run also blocked for symmetry)" {
  run bash "$HOOK" <<< "$(make_input 'git clean -n')"
  [ "$status" -eq 2 ]
}

@test "blocks git stash drop" {
  run bash "$HOOK" <<< "$(make_input 'git stash drop')"
  [ "$status" -eq 2 ]
}

@test "blocks git stash clear" {
  run bash "$HOOK" <<< "$(make_input 'git stash clear')"
  [ "$status" -eq 2 ]
}

@test "blocks git branch -D" {
  run bash "$HOOK" <<< "$(make_input 'git branch -D feature/foo')"
  [ "$status" -eq 2 ]
}

@test "allows git branch -d (safe delete)" {
  run bash "$HOOK" <<< "$(make_input 'git branch -d feature/foo')"
  [ "$status" -eq 0 ]
}

@test "blocks git worktree remove" {
  run bash "$HOOK" <<< "$(make_input 'git worktree remove ../foo')"
  [ "$status" -eq 2 ]
}

@test "allows git worktree list" {
  run bash "$HOOK" <<< "$(make_input 'git worktree list')"
  [ "$status" -eq 0 ]
}

@test "blocks git filter-branch" {
  run bash "$HOOK" <<< "$(make_input 'git filter-branch --env-filter foo HEAD')"
  [ "$status" -eq 2 ]
}

@test "blocks git filter-repo" {
  run bash "$HOOK" <<< "$(make_input 'git filter-repo --invert-paths --path foo')"
  [ "$status" -eq 2 ]
}

@test "blocks git replace" {
  run bash "$HOOK" <<< "$(make_input 'git replace abc def')"
  [ "$status" -eq 2 ]
}

@test "blocks git reflog delete" {
  run bash "$HOOK" <<< "$(make_input 'git reflog delete refs/heads/foo@{0}')"
  [ "$status" -eq 2 ]
}

@test "blocks git reflog expire" {
  run bash "$HOOK" <<< "$(make_input 'git reflog expire --expire=now --all')"
  [ "$status" -eq 2 ]
}

@test "allows git reflog (default subcommand show)" {
  run bash "$HOOK" <<< "$(make_input 'git reflog')"
  [ "$status" -eq 0 ]
}

@test "blocks git symbolic-ref --delete HEAD" {
  run bash "$HOOK" <<< "$(make_input 'git symbolic-ref --delete HEAD')"
  [ "$status" -eq 2 ]
}

@test "blocks git symbolic-ref -d HEAD" {
  run bash "$HOOK" <<< "$(make_input 'git symbolic-ref -d HEAD')"
  [ "$status" -eq 2 ]
}

@test "allows git symbolic-ref HEAD (read)" {
  run bash "$HOOK" <<< "$(make_input 'git symbolic-ref HEAD')"
  [ "$status" -eq 0 ]
}

@test "blocks git gc --prune=now" {
  run bash "$HOOK" <<< "$(make_input 'git gc --prune=now')"
  [ "$status" -eq 2 ]
}

@test "blocks git gc --prune now (separate token)" {
  run bash "$HOOK" <<< "$(make_input 'git gc --prune now')"
  [ "$status" -eq 2 ]
}

@test "allows plain git gc" {
  run bash "$HOOK" <<< "$(make_input 'git gc')"
  [ "$status" -eq 0 ]
}

# --- Blocked: argv-aware destructive variants ---

@test "blocks git reset --hard" {
  run bash "$HOOK" <<< "$(make_input 'git reset --hard HEAD')"
  [ "$status" -eq 2 ]
}

@test "blocks git reset HEAD~1 --hard (flag at end)" {
  run bash "$HOOK" <<< "$(make_input 'git reset HEAD~1 --hard')"
  [ "$status" -eq 2 ]
}

@test "blocks git reset --keep" {
  run bash "$HOOK" <<< "$(make_input 'git reset --keep HEAD~1')"
  [ "$status" -eq 2 ]
}

@test "blocks git reset --merge" {
  run bash "$HOOK" <<< "$(make_input 'git reset --merge')"
  [ "$status" -eq 2 ]
}

@test "allows git reset --soft" {
  run bash "$HOOK" <<< "$(make_input 'git reset --soft HEAD~1')"
  [ "$status" -eq 0 ]
}

@test "allows git reset --mixed" {
  run bash "$HOOK" <<< "$(make_input 'git reset --mixed HEAD~1')"
  [ "$status" -eq 0 ]
}

@test "allows git reset HEAD <file> (default --mixed unstage)" {
  run bash "$HOOK" <<< "$(make_input 'git reset HEAD foo.py')"
  [ "$status" -eq 0 ]
}

@test "blocks git checkout -- foo.py" {
  run bash "$HOOK" <<< "$(make_input 'git checkout -- foo.py')"
  [ "$status" -eq 2 ]
}

@test "blocks git checkout ." {
  run bash "$HOOK" <<< "$(make_input 'git checkout .')"
  [ "$status" -eq 2 ]
}

@test "blocks git checkout -f main" {
  run bash "$HOOK" <<< "$(make_input 'git checkout -f main')"
  [ "$status" -eq 2 ]
}

@test "blocks git checkout --force main" {
  run bash "$HOOK" <<< "$(make_input 'git checkout --force main')"
  [ "$status" -eq 2 ]
}

@test "blocks git checkout -B existing" {
  run bash "$HOOK" <<< "$(make_input 'git checkout -B existing')"
  [ "$status" -eq 2 ]
}

@test "allows plain git checkout main" {
  run bash "$HOOK" <<< "$(make_input 'git checkout main')"
  [ "$status" -eq 0 ]
}

@test "allows git checkout -b new" {
  run bash "$HOOK" <<< "$(make_input 'git checkout -b new')"
  [ "$status" -eq 0 ]
}

@test "blocks git restore foo.py" {
  run bash "$HOOK" <<< "$(make_input 'git restore foo.py')"
  [ "$status" -eq 2 ]
}

@test "allows git restore --staged foo.py" {
  run bash "$HOOK" <<< "$(make_input 'git restore --staged foo.py')"
  [ "$status" -eq 0 ]
}

@test "blocks git restore --staged --worktree foo.py" {
  run bash "$HOOK" <<< "$(make_input 'git restore --staged --worktree foo.py')"
  [ "$status" -eq 2 ]
}

@test "blocks git restore --source HEAD~1 foo.py" {
  run bash "$HOOK" <<< "$(make_input 'git restore --source HEAD~1 foo.py')"
  [ "$status" -eq 2 ]
}

@test "blocks git switch --discard-changes main" {
  run bash "$HOOK" <<< "$(make_input 'git switch --discard-changes main')"
  [ "$status" -eq 2 ]
}

@test "blocks git switch -f main" {
  run bash "$HOOK" <<< "$(make_input 'git switch -f main')"
  [ "$status" -eq 2 ]
}

@test "blocks git switch --force main" {
  run bash "$HOOK" <<< "$(make_input 'git switch --force main')"
  [ "$status" -eq 2 ]
}

@test "blocks git switch -C feature/foo" {
  run bash "$HOOK" <<< "$(make_input 'git switch -C feature/foo')"
  [ "$status" -eq 2 ]
}

@test "allows plain git switch" {
  run bash "$HOOK" <<< "$(make_input 'git switch main')"
  [ "$status" -eq 0 ]
}

@test "allows git switch -c new" {
  run bash "$HOOK" <<< "$(make_input 'git switch -c feature/foo')"
  [ "$status" -eq 0 ]
}

@test "blocks git update-ref -d refs/heads/foo" {
  run bash "$HOOK" <<< "$(make_input 'git update-ref -d refs/heads/foo')"
  [ "$status" -eq 2 ]
}

@test "blocks git update-ref --no-deref -d refs/heads/foo (flag interleaved)" {
  run bash "$HOOK" <<< "$(make_input 'git update-ref --no-deref -d refs/heads/foo')"
  [ "$status" -eq 2 ]
}

@test "allows git update-ref refs/heads/foo SHA (non-delete write)" {
  run bash "$HOOK" <<< "$(make_input 'git update-ref refs/heads/foo abc123')"
  [ "$status" -eq 0 ]
}

# --- Wrapper / chain coverage for destroy ---

@test "blocks bash -c with git rm" {
  run bash "$HOOK" <<< "$(make_input 'bash -c "git rm foo.py"')"
  [ "$status" -eq 2 ]
}

@test "blocks chained git clean" {
  run bash "$HOOK" <<< "$(make_input 'cd /tmp && git clean -fd')"
  [ "$status" -eq 2 ]
}

@test "blocks chained git reset --hard" {
  run bash "$HOOK" <<< "$(make_input 'git status; git reset --hard HEAD')"
  [ "$status" -eq 2 ]
}

@test "blocks bash -c with git update-ref -d" {
  run bash "$HOOK" <<< "$(make_input 'bash -c "git update-ref -d refs/heads/foo"')"
  [ "$status" -eq 2 ]
}

@test "blocks chained git checkout -- after cd" {
  run bash "$HOOK" <<< "$(make_input 'cd /tmp && git checkout -- foo.py')"
  [ "$status" -eq 2 ]
}
