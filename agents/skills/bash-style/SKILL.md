---
name: bash-style
description: >
    Bash script coding conventions and bats testing: shebang (#!/bin/bash), set -euxCo pipefail, cd "$(dirname "$0")", usage function with heredoc, readonly constants, SCREAMING_SNAKE_CASE constants, snake_case variables/functions, _snake_case local variables, bats test structure (test-helper/, setup/teardown lifecycle, run/status/output assertions). Load whenever writing, reviewing, or refactoring any Bash script (.sh files, shell scripts) or bats test (.bats files) — new files, bug fixes, function design, project setup, writing tests. Also load when PLANNING or DISCUSSING Bash/shell script implementation or test design, even before any code is written. Without this skill, you will use wrong conventions (missing set flags, wrong naming, no usage function, no readonly, wrong test directory structure) that this user explicitly does not want.
---

# Shell Script Coding Style Guidelines

## File Header (every script)

Every shell script must start with these three lines in order:

```sh
#!/bin/bash
set -euxCo pipefail
cd "$(dirname "$0")"
```

- `#!/bin/bash` — run with bash explicitly, not sh
- `set -euxCo pipefail`:
    - `-e`: exit on error
    - `-u`: exit on undefined variable reference
    - `-x`: print each command to stderr before execution (debug mode)
    - `-C`: prohibit overwriting files with `>` (use `>|` to force)
    - `-o pipefail`: fail if any command in a pipe chain fails
- `cd "$(dirname "$0")"` — change to the script's own directory so relative paths work regardless of where the caller invoked the script from

To suppress debug output for a section, bracket it:

```sh
set +x
# ... noisy or sensitive section ...
set -x
```

## usage Function

Every script must define a `usage` function that prints documentation to stderr and exits with failure. Use heredoc + redirect inside the function body:

```sh
function usage() {
  cat <<EOF >&2
Description:
    Description of this script.

Usage:
    $0 [OPTIONS] <FILE>

Options:
    --version, -v: print "$(basename "$0")" version
    --help, -h: print this
EOF
  exit 1
}
```

Call `usage` for `--help`/`-h` flags and for invalid argument combinations.

## Constants

Declare constants with `readonly`. Names use `SCREAMING_SNAKE_CASE`:

```sh
readonly INPUT_DIR="../data/input"
readonly MAX_RETRY=3
```

Always quote the right-hand side — values may contain spaces or special characters.

## Variables

- Quote all parameter expansions and string values to guard against word splitting and glob expansion (shellcheck SC2086 catches unquoted uses)
- Use default values for variables that may be empty or undefined:
    ```sh
    readonly OUTPUT="${1:-output.txt}"   # default value
    readonly NAME="${NAME:?NAME is required}"  # error if unset/empty
    ```
- Define variables just before first use (minimize lifetime)

## Naming

| Kind | Convention | Example |
| --- | --- | --- |
| Constants | `SCREAMING_SNAKE_CASE` + `readonly` | `readonly MAX_RETRY=3` |
| Variables | `snake_case` | `input_file="..."` |
| Functions | `snake_case` | `function parse_args()` |
| Local variables | `_snake_case` (leading underscore) | `local _tmp_dir` |
| Files | `snake_case` | `lint_format.sh` |
| Directories | `kebab-case` | `claude-scripts/` |

Do not start names with a digit.

## Local Variables

Declare function-local variables with `local` and prefix with `_`:

```sh
function build_output() {
  local _src="$1"
  local _dst="$2"
  cp "$_src" "$_dst"
}
```

## Formatting

Indent with **2 spaces** (Google Shell Style Guide convention). Configure via `.editorconfig` at the project root; `shfmt` reads it automatically. Run `shfmt -i 2 -d <file>` to check. Common quoting errors are caught by `shellcheck`. The conventions above describe what those tools do not enforce.

## Full Template

```sh
#!/bin/bash
set -euxCo pipefail
cd "$(dirname "$0")"

function usage() {
  cat <<EOF >&2
Description:
    Does X given Y.

Usage:
    $0 [OPTIONS] <INPUT>

Options:
    --dry-run: print actions without executing
    --help, -h: print this
EOF
  exit 1
}

readonly LOG_DIR="../logs"

function main() {
  local _input="${1:?$(usage)}"

  set +x
  echo "processing: $_input"
  set -x

  # ... implementation ...
}

main "$@"
```

---

## Testing with bats

Test shell scripts with [bats](https://github.com/bats-core/bats-core) (Bash Automated Testing System). Available tools: `bats`, `shellcheck`, `shfmt`.

### Directory Layout

```
tests/
├── <category>/
│   ├── test-helper/
│   │   └── setup.bash          # shared fixtures and helpers for this category
│   ├── feature_a.bats
│   └── feature_b.bats
└── <category>/
    ├── test-helper/
    │   └── setup.bash
    └── another.bats
```

- `test-helper/` is the project-standard directory name for helper libraries
- Each category gets its own `test-helper/` because helpers are domain-specific (hook input builders differ from CLI stubs)
- Test files: `.bats` extension, `snake_case` naming
- Helper files: `.bash` extension, loaded via `load test-helper/setup`

### File Structure

Every `.bats` file follows this order:

```bash
#!/usr/bin/env bats
# One-line description of what this file tests.

setup() {
  load test-helper/setup
  SCRIPT="$REPO_ROOT/path/to/script_under_test.sh"
}

@test "descriptive lowercase sentence" {
  run bash "$SCRIPT" --help
  [ "$status" -eq 1 ]
  [[ "$output" == *"Usage:"* ]]
}
```

- Shebang: `#!/usr/bin/env bats` (not `#!/bin/bash`)
- One comment line describing the file's scope
- `setup()` runs before each `@test` — wire fixtures and paths here, not assertions
- When a category has no shared helpers, derive `REPO_ROOT` inline:
    ```bash
    setup() {
      REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
    }
    ```

### Setup / Teardown Lifecycle

bats provides four lifecycle hooks, from broadest to narrowest scope:

| Hook | Scope | Defined in |
| --- | --- | --- |
| `setup_suite` / `teardown_suite` | Entire test run | `setup_suite.bash` (auto-discovered at test root) |
| `setup_file` / `teardown_file` | Per `.bats` file | The `.bats` file itself |
| `setup` / `teardown` | Per `@test` | The `.bats` file itself |

Use the narrowest scope that fits:

- `setup` — default choice; cheap per-test wiring (variable assignment, path setup)
- `setup_file` — expensive one-time setup shared across tests in a file (temp git repos, compiled fixtures)
- `setup_suite` — global preconditions (tool availability checks, environment validation)

```bash
setup_file() {
  load test-helper/setup
  create_temp_git_repo   # expensive — do once per file
  export TEMP_REPO       # export so tests see it
}

teardown_file() {
  rm -rf "$TEMP_REPO"
}

setup() {
  load test-helper/setup
  HOOK="$HOOK_DIR/my_hook.sh"
}
```

Variables set in `setup_file` must be `export`ed to be visible in `setup` and `@test` blocks (they run in subshells).

### Test Naming

Use `@test "descriptive lowercase sentence"` — describe the observable behavior, not the implementation:

```bash
@test "allows push to feature branch" { ... }
@test "blocks --force" { ... }
@test "exits 0 when no file_path in input" { ... }
```

### Section Separators

Group related tests with comment banners:

```bash
# ============================================================
# Allowed: normal pushes
# ============================================================
```

### Assertions

Use raw bats built-ins (`run`, `$status`, `$output`, `${lines[@]}`):

```bash
run bash "$SCRIPT" "$arg"
[ "$status" -eq 0 ]                    # exit status
[[ "$output" == *"expected text"* ]]   # output substring
[ "${lines[0]}" = "first line" ]       # exact line
[ "${#lines[@]}" -eq 3 ]              # line count
! [[ "$output" == *"unexpected"* ]]    # negation
```

Always use `run` before checking `$status` or `$output` — without it, a non-zero exit aborts the test (bats enables `set -e` inside `@test` blocks).

When a command is expected to succeed and you only care about side effects (a file was created, a variable was set), omit `run` — bats' `set -e` will fail the test automatically if the command errors.

### Loop-Based Assertions

When checking multiple items in a loop, report which item failed:

```bash
@test "all scripts pass syntax check" {
  for script in "${SCRIPTS[@]}"; do
    bash -n "$script" || {
      echo "Syntax error in: $script"
      return 1
    }
  done
}
```

### Helper Functions (test-helper/setup.bash)

Put shared fixtures in `test-helper/setup.bash`. Common patterns:

```bash
REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HOOK_DIR="$REPO_ROOT/agents/hooks"

make_input() {
  jq -n --arg cmd "$1" '{"tool_input":{"command":$cmd}}'
}

create_temp_git_repo() {
  TEMP_REPO="$(mktemp -d "${BATS_TEST_TMPDIR:-/tmp}/repo.XXXXXX")"
  git -C "$TEMP_REPO" init --quiet
  git -C "$TEMP_REPO" config user.email "test@test.com"
  git -C "$TEMP_REPO" config user.name "Test"
  git -C "$TEMP_REPO" config commit.gpgsign false
  touch "$TEMP_REPO/.gitkeep"
  git -C "$TEMP_REPO" add .gitkeep
  git -C "$TEMP_REPO" commit --quiet -m "initial"
}
```

- Use `jq` for JSON construction (proper escaping)
- Build helpers that mirror the input format of the code under test
- For external command stubs (e.g. `gh`), create an executable script in `$BATS_TEST_TMPDIR/bin/` and prepend it to `$PATH`

### Temporary Directories

bats provides auto-cleaned temp directories at three scopes:

| Variable            | Scope                 | Cleaned after |
| ------------------- | --------------------- | ------------- |
| `BATS_TEST_TMPDIR`  | Per `@test`           | Each test     |
| `BATS_FILE_TMPDIR`  | Per `.bats` file      | Each file     |
| `BATS_SUITE_TMPDIR` | Per `bats` invocation | Entire run    |

Use the narrowest scope that fits. Prefer these over raw `mktemp` — bats handles cleanup.

### Making Scripts Testable

To source individual functions from a script without executing `main`, guard the entry point:

```bash
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main "$@"
fi
```

This lets tests `source` the script and call functions in isolation.

### Running Tests

```bash
bats tests/                        # all tests
bats tests/hooks/claude/           # one category
bats tests/hooks/claude/guard_dangerous_git.bats  # one file
bats --filter "blocks" tests/      # only tests matching pattern
bats --negative-filter "slow" tests/  # exclude tests matching pattern
```

### Test Organization Rules

- One `.bats` file per script or module under test
- Test allowed/passing cases first, then blocked/failing cases
- Each `@test` tests one behavior — split combined checks into separate tests
- Keep test bodies short; move complex setup into helper functions
