#!/bin/bash
# Claude Code PreToolUse hook: block destructive / review-bypassing git operations.
#
# Covered:
#   - git push --force / -f / --force-with-lease     (force push)
#   - git push ... +refspec                          (force push via + prefix)
#   - git push to main/master (explicit or implicit via current branch)
#   - gh pr merge --admin                            (bypasses review)
#   - git rm / git clean / git stash drop|clear      (always destructive)
#   - git branch -D                                  (force branch delete)
#   - git worktree remove                            (worktree delete)
#   - git filter-branch / filter-repo / replace      (history rewrite)
#   - git reflog delete | expire                     (loses recovery records)
#   - git symbolic-ref --delete | -d                 (HEAD/ref delete)
#   - git update-ref -d                              (plumbing ref delete)
#   - git gc --prune                                 (forces unreachable cleanup)
#   - git reset --hard | --keep | --merge            (working-tree destruction)
#   - git checkout with -f / --force / -B / -- / .   (working-tree destruction)
#   - git restore <file> without sole --staged       (working-tree destruction)
#   - git switch -f / --force / -C / --discard-changes
#
# Exit code 0 = allow, exit code 2 = block.

set -euCo pipefail

# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/shell_parse.sh"
# shellcheck disable=SC1091
source "$(dirname "${BASH_SOURCE[0]}")/lib/audit_log.sh"

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
SESSION=$(echo "$INPUT" | jq -r '.session_id // empty')

# Detect shell-wrapper commands that execute an arbitrary inner string
# (bash -c, sh -c, zsh -c, the e-v-a-l builtin). This hook cannot statically
# parse the inner shell, so we fail closed when the wrapper carries tokens
# that could indicate a dangerous git operation.
function wrapper_unsafe() {
  local _seg="$1"
  # Match `c` anywhere in a combined short-option cluster so that forms
  # like `bash -xc`, `bash -cx`, `bash -evc` all get caught, not just the
  # trailing-c case.
  if ! printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])((/bin/|/usr/bin/)?(ba|z)?sh[[:space:]]+-[^[:space:]]*c[^[:space:]]*([[:space:]]|$)|eval([[:space:]]|$))'; then
    return 1
  fi
  if printf '%s\n' "$_seg" | grep -qE '(git[[:space:]]+push|gh[[:space:]]+pr[[:space:]]+merge|--force|--force-with-lease|[[:space:]]\+?(main|master)([[:space:]]|$)|--admin|git[[:space:]]+(rm|clean|filter-branch|filter-repo|replace|update-ref|symbolic-ref|restore|reflog|worktree|checkout)([[:space:]]|$)|git[[:space:]]+stash[[:space:]]+(drop|clear)([[:space:]]|$)|git[[:space:]]+branch[[:space:]]+(.*[[:space:]])?-D([[:space:]]|$)|git[[:space:]]+gc([[:space:]]|$)|(--hard|--keep|--discard-changes|--prune)([[:space:]]|=|$)|[[:space:]]-B([[:space:]]|$))'; then
    return 0
  fi
  return 1
}

# Analyze a `git push` segment. Returns 2 if dangerous, 0 otherwise.
# Parses options robustly: handles `--`, flags with a separate argument
# (-u / --set-upstream / --repo / --receive-pack / --exec), flag=value
# forms, and multiple refspecs.
function analyze_git_push() {
  local _seg="$1"
  local _rest _current_branch _remote _refspec _target
  local -a _raw_tokens _positional _refspecs
  local _i _token

  # --- --force / --force-with-lease ---
  if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(-f|--force|--force-with-lease)([[:space:]]|=|$)'; then
    cat >&2 <<ERRMSG
BLOCKED: force-push flag detected.

Segment: $_seg

Why: Force push rewrites history and can destroy teammates' commits. Even
     --force-with-lease has a narrow safety window; deny by default.

What to do:
  Claude Code: Push without --force. If history rewrite is genuinely required,
               ask the user to run it manually.
  User: Run 'git push --force' manually if you decide it is needed.
ERRMSG
    return 2
  fi

  # --- Parse positional args ---
  _rest=$(printf '%s\n' "$_seg" | sed -E 's/^[[:space:]]*git[[:space:]]+push([[:space:]]|$)//; s/[[:space:]]+(2>&1|2>\/dev\/null|>&2)[[:space:]]*$//; s/[[:space:]]+$//')

  _raw_tokens=()
  _positional=()
  _refspecs=()

  IFS=' ' read -r -a _raw_tokens <<<"$_rest"

  _i=0
  while [ "$_i" -lt "${#_raw_tokens[@]}" ]; do
    _token="${_raw_tokens[$_i]}"
    if [ -z "$_token" ]; then
      _i=$((_i + 1))
      continue
    fi
    case "$_token" in
      --)
        _i=$((_i + 1))
        while [ "$_i" -lt "${#_raw_tokens[@]}" ]; do
          [ -n "${_raw_tokens[$_i]}" ] && _positional+=("${_raw_tokens[$_i]}")
          _i=$((_i + 1))
        done
        break
        ;;
      --repo | --receive-pack | --exec)
        # These flags consume the next token as their argument.
        _i=$((_i + 2))
        ;;
      -u | --set-upstream)
        # Boolean flags; do not consume the next token.
        _i=$((_i + 1))
        ;;
      --*=* | -*)
        _i=$((_i + 1))
        ;;
      *)
        _positional+=("$_token")
        _i=$((_i + 1))
        ;;
    esac
  done

  # First positional is remote, rest are refspecs.
  _remote=""
  [ "${#_positional[@]}" -ge 1 ] && _remote="${_positional[0]}"
  [ "${#_positional[@]}" -ge 2 ] && _refspecs=("${_positional[@]:1}")

  # --- Implicit target (no refspec): use current branch ---
  if [ "${#_refspecs[@]}" -eq 0 ]; then
    _current_branch=$(git branch --show-current 2>/dev/null || printf '')
    _current_branch="${_current_branch#refs/heads/}"
    if [ "$_current_branch" = "main" ] || [ "$_current_branch" = "master" ]; then
      cat >&2 <<ERRMSG
BLOCKED: push target is '$_current_branch'.

Segment: $_seg

Why: All changes must go through a pull request. Direct push to
     '$_current_branch' bypasses review and may trigger production CI
     unexpectedly.

What to do:
  Claude Code: Switch to a feature branch and open a PR via 'gh pr create'.
  User: If a direct push is truly needed (emergency revert, etc.), run it
        manually in your terminal.
ERRMSG
      return 2
    fi
    return 0
  fi

  # --- Explicit refspecs: check each ---
  for _refspec in "${_refspecs[@]}"; do
    # +refspec force push
    if [[ "$_refspec" == +* ]]; then
      cat >&2 <<ERRMSG
BLOCKED: '+refspec' force push detected.

Segment: $_seg

Why: The '+' prefix on a refspec forces a non-fast-forward update,
     equivalent to --force for that refspec.

What to do:
  Claude Code: Push without the '+' prefix.
  User: Run the force push manually if you decide it is needed.
ERRMSG
      return 2
    fi

    _target=""
    if [[ "$_refspec" == *:* ]]; then
      _target="${_refspec##*:}"
    elif [ "$_refspec" = "HEAD" ]; then
      _target=$(git branch --show-current 2>/dev/null || printf '')
    else
      _target="$_refspec"
    fi
    _target="${_target#refs/heads/}"

    [ -z "$_target" ] && continue

    if [ "$_target" = "main" ] || [ "$_target" = "master" ]; then
      cat >&2 <<ERRMSG
BLOCKED: push target is '$_target'.

Segment: $_seg

Why: All changes must go through a pull request. Direct push to '$_target'
     bypasses review and may trigger production CI unexpectedly.

What to do:
  Claude Code: Switch to a feature branch and open a PR via 'gh pr create'.
  User: If a direct push is truly needed (emergency revert, etc.), run it
        manually in your terminal.
ERRMSG
      return 2
    fi
  done

  return 0
}

# Emit a block message for a destructive non-push git operation.
function _emit_destroy_block() {
  local _seg="$1"
  local _why="$2"
  cat >&2 <<ERRMSG
BLOCKED: destructive git operation.

Segment: $_seg

Why: $_why.

What to do:
  Claude Code: Stop and ask the user, or pick a non-destructive alternative
               (e.g., 'git restore --staged <file>' to unstage, 'git switch
               <branch>' for branch changes, 'git stash push' to save WIP).
  User: Run the destructive command manually if you decide it is needed.
ERRMSG
}

# Analyze a segment for non-push destructive git operations. The push path is
# handled separately by analyze_git_push because its risk model is different
# (force / protected branches) and the parser there is heavier. Here we cover
# the local-state destruction surface: working-tree wipes, ref deletions,
# stash drops, history rewrites, and reflog/gc operations that erase the
# recovery path. Returns 2 if dangerous, 0 otherwise.
function analyze_git_destroy() {
  local _seg="$1"

  # --- Always-destructive verbs (any argv shape) ---
  # These operations destroy local state regardless of how they're invoked,
  # so prefix detection is sufficient. We deliberately also block dry-run
  # forms (e.g. 'git clean -n') to keep this surface uniformly off-limits;
  # if a user wants to inspect what *would* be removed, they can run it
  # manually.
  local -a _always_destructive=(
    '^git[[:space:]]+rm([[:space:]]|$)'
    '^git[[:space:]]+clean([[:space:]]|$)'
    '^git[[:space:]]+filter-branch([[:space:]]|$)'
    '^git[[:space:]]+filter-repo([[:space:]]|$)'
    '^git[[:space:]]+replace([[:space:]]|$)'
    '^git[[:space:]]+stash[[:space:]]+(drop|clear)([[:space:]]|$)'
    '^git[[:space:]]+branch[[:space:]]+(.*[[:space:]])?-D([[:space:]]|$)'
    '^git[[:space:]]+worktree[[:space:]]+remove([[:space:]]|$)'
    '^git[[:space:]]+reflog[[:space:]]+(delete|expire)([[:space:]]|$)'
    '^git[[:space:]]+symbolic-ref[[:space:]]+(.*[[:space:]])?(--delete|-d)([[:space:]]|=|$)'
    '^git[[:space:]]+gc[[:space:]]+(.*[[:space:]])?--prune([[:space:]]|=|$)'
  )
  local _pat
  for _pat in "${_always_destructive[@]}"; do
    if printf '%s\n' "$_seg" | grep -qE "$_pat"; then
      _emit_destroy_block "$_seg" \
        "this verb / flag combination is always destructive (rm, clean, filter-*, replace, stash drop|clear, branch -D, worktree remove, reflog delete|expire, symbolic-ref --delete, gc --prune)"
      return 2
    fi
  done

  # --- git reset --hard / --keep / --merge ---
  # --soft (HEAD only) and --mixed (default; HEAD + index) preserve the
  # working tree, so they are safe to allow. The destructive trio is detected
  # regardless of position because users sometimes write 'git reset HEAD~1
  # --hard' with the flag trailing.
  if printf '%s\n' "$_seg" | grep -qE '^git[[:space:]]+reset([[:space:]]|$)'; then
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(--hard|--keep|--merge)([[:space:]]|=|$)'; then
      _emit_destroy_block "$_seg" \
        "git reset --hard / --keep / --merge can destroy uncommitted work in the working tree; use --soft or --mixed instead, or ask the user"
      return 2
    fi
  fi

  # --- git restore: only sole --staged is allowed ---
  # 'git restore --staged <file>' just unstages and is the inverse of
  # 'git add'. Any other form ('git restore <file>', '--worktree',
  # '--source=<commit>') overwrites uncommitted work.
  if printf '%s\n' "$_seg" | grep -qE '^git[[:space:]]+restore([[:space:]]|$)'; then
    local _has_staged=false _has_worktree=false _has_source=false
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(--staged|-S)([[:space:]]|=|$)'; then
      _has_staged=true
    fi
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(--worktree|-W)([[:space:]]|=|$)'; then
      _has_worktree=true
    fi
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(--source|-s)([[:space:]]|=|$)'; then
      _has_source=true
    fi
    if ! { [ "$_has_staged" = true ] && [ "$_has_worktree" = false ] && [ "$_has_source" = false ]; }; then
      _emit_destroy_block "$_seg" \
        "git restore <file> overwrites uncommitted work; only 'git restore --staged <file>' (alone, without --worktree / --source) is allowed"
      return 2
    fi
  fi

  # --- git switch -f / --force / -C / --discard-changes ---
  # 'git switch <branch>' and '-c <new>' are safe (git refuses to clobber
  # working-tree changes). The destructive variants drop those guards.
  if printf '%s\n' "$_seg" | grep -qE '^git[[:space:]]+switch([[:space:]]|$)'; then
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(-f|--force|-C|--discard-changes)([[:space:]]|=|$)'; then
      _emit_destroy_block "$_seg" \
        "git switch -f / --force / -C / --discard-changes overwrites uncommitted work or branch refs"
      return 2
    fi
  fi

  # --- git checkout: branch-switch only; refuse -f / --force / -B / -- / . ---
  # 'git checkout' overloads branch switching with file restore. Without
  # branch-name probing, we can't disambiguate 'git checkout foo' (branch
  # vs. file). We therefore allow simple forms and refuse the syntactic
  # markers that *guarantee* working-tree destruction:
  #   -f / --force        : drops working-tree changes
  #   -B                  : overwrites an existing branch ref
  #   --                  : the path-spec marker — anything after is files
  #   .                   : restore everything in CWD
  # Users wanting file restore should use 'git restore --staged' (unstage)
  # or run the destructive form manually.
  if printf '%s\n' "$_seg" | grep -qE '^git[[:space:]]+checkout([[:space:]]|$)'; then
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(-f|--force|-B)([[:space:]]|=|$)' ||
      printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])--([[:space:]]|$)' ||
      printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])\.([[:space:]]|$)'; then
      _emit_destroy_block "$_seg" \
        "destructive 'git checkout' (-f / --force / -B / -- <file> / .); use 'git switch <branch>' for branch changes and 'git restore --staged <file>' for unstaging"
      return 2
    fi
  fi

  # --- git update-ref -d / --delete (any position) ---
  # Plumbing-level ref deletion. Equivalent to 'git branch -D' but bypasses
  # the merge-ahead check that 'git branch -d' performs.
  if printf '%s\n' "$_seg" | grep -qE '^git[[:space:]]+update-ref([[:space:]]|$)'; then
    if printf '%s\n' "$_seg" | grep -qE '(^|[[:space:]])(-d|--delete)([[:space:]]|=|$)'; then
      _emit_destroy_block "$_seg" \
        "git update-ref -d deletes refs at the plumbing layer, equivalent to a forced branch deletion that bypasses the merge-ahead check that 'git branch -d' performs"
      return 2
    fi
  fi

  return 0
}

# Per-segment analysis. Returns 2 if segment is dangerous, 0 otherwise.
function analyze_segment() {
  local _seg="$1"
  local _trimmed
  _trimmed=$(printf '%s\n' "$_seg" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')
  [ -z "$_trimmed" ] && return 0

  # --- shell wrapper carrying destructive tokens (fail closed) ---
  if wrapper_unsafe "$_trimmed"; then
    cat >&2 <<ERRMSG
BLOCKED: shell wrapper (bash -c / sh -c / the e-v-a-l builtin) with destructive git tokens.

Segment: $_trimmed

Why: This hook cannot statically analyze commands run through a shell wrapper,
     and the segment carries tokens suggesting a destructive operation
     (force / admin push, push to main|master, git rm / clean / filter-* /
     replace / update-ref / symbolic-ref / restore / reflog / worktree /
     checkout, git stash drop|clear, git branch -D, git gc, or destructive
     flags such as --hard / --keep / --discard-changes / --prune / -B).
     Failing closed.

What to do:
  Claude Code: Rewrite without the shell wrapper so the command is directly
               visible, or ask the user to run it manually.
  User: Run the command manually in your terminal if it is safe.
ERRMSG
    return 2
  fi

  # --- gh pr merge --admin ---
  if printf '%s\n' "$_trimmed" | grep -qE '^gh[[:space:]]+pr[[:space:]]+merge([[:space:]]|$)' &&
    printf '%s\n' "$_trimmed" | grep -qE '(^|[[:space:]])--admin([[:space:]]|=|$)'; then
    cat >&2 <<ERRMSG
BLOCKED: 'gh pr merge --admin' bypasses required reviews and branch protection.

Segment: $_trimmed

Why: Admin merge is reserved for genuine emergencies and should be a human
     decision, not an agent decision.

What to do:
  Claude Code: Merge without --admin. If admin merge is genuinely required,
               stop and ask the user.
  User: Run 'gh pr merge --admin' manually if you decide it is needed.
ERRMSG
    return 2
  fi

  # --- git push analysis ---
  if printf '%s\n' "$_trimmed" | grep -qE '^git[[:space:]]+push([[:space:]]|$)'; then
    analyze_git_push "$_trimmed"
    return $?
  fi

  # --- destructive non-push git operations ---
  analyze_git_destroy "$_trimmed"
  return $?
}

# Iterate over segments; block on first dangerous match.
# Capture analyze_segment's stderr so the audit log can record the first
# "BLOCKED: ..." line as the reason; the captured text is then replayed to
# the real stderr so the user-facing message is unchanged.
while IFS= read -r segment; do
  if err=$(analyze_segment "$segment" 2>&1); then
    continue
  fi
  reason=$(printf '%s\n' "$err" | head -n 1)
  log_blocked Bash "$COMMAND" "$reason" guard_dangerous_git.sh "$SESSION"
  printf '%s\n' "$err" >&2
  exit 2
done <<<"$(split_command_segments "$COMMAND")"

exit 0
