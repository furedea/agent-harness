#!/usr/bin/env bats
# Tests the release installer script without network access.

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/install.sh"
  TEST_ROOT="$BATS_TEST_TMPDIR/root"
  FAKE_BIN="$BATS_TEST_TMPDIR/bin"
  ARCHIVE="$BATS_TEST_TMPDIR/agent-harness.tar.gz"
  URL_LOG="$BATS_TEST_TMPDIR/url.log"
  HARNESS_LOG="$BATS_TEST_TMPDIR/harness.log"
  mkdir -p "$TEST_ROOT" "$FAKE_BIN"
  make_fake_archive
  make_fake_curl
}

@test "installs latest release archive and runs install verification" {
  AGENT_HARNESS_INSTALL_DIR="$TEST_ROOT/.local/agent-harness" \
    AGENT_HARNESS_BIN_DIR="$TEST_ROOT/.local/bin" \
    AGENT_HARNESS_PREFIX="$TEST_ROOT" \
    AGENT_HARNESS_TEST_LOG="$HARNESS_LOG" \
    FAKE_ARCHIVE="$ARCHIVE" \
    URL_LOG="$URL_LOG" \
    PATH="$FAKE_BIN:$PATH" \
    run bash "$SCRIPT"

  [ "$status" -eq 0 ]
  [ -x "$TEST_ROOT/.local/agent-harness/agent-harness" ]
  [ -d "$TEST_ROOT/.local/agent-harness/share" ]
  [ -L "$TEST_ROOT/.local/bin/agent-harness" ]
  grep -q "install --prefix $TEST_ROOT" "$HARNESS_LOG"
  grep -q "verify --prefix $TEST_ROOT" "$HARNESS_LOG"
  grep -q "https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.gz" "$URL_LOG"
}

@test "uses explicit release tag when AGENT_HARNESS_VERSION is set" {
  AGENT_HARNESS_INSTALL_DIR="$TEST_ROOT/.local/agent-harness" \
    AGENT_HARNESS_BIN_DIR="$TEST_ROOT/.local/bin" \
    AGENT_HARNESS_PREFIX="$TEST_ROOT" \
    AGENT_HARNESS_VERSION="agent-harness-v9.9.9" \
    AGENT_HARNESS_TEST_LOG="$HARNESS_LOG" \
    FAKE_ARCHIVE="$ARCHIVE" \
    URL_LOG="$URL_LOG" \
    PATH="$FAKE_BIN:$PATH" \
    run bash "$SCRIPT"

  [ "$status" -eq 0 ]
  grep -q "https://github.com/furedea/agent-harness/releases/download/agent-harness-v9.9.9/agent-harness-x86_64-unknown-linux-musl.tar.gz" "$URL_LOG"
}

make_fake_archive() {
  local _package_dir="$BATS_TEST_TMPDIR/package/agent-harness-x86_64-unknown-linux-musl"
  mkdir -p "$_package_dir/share/agent-harness"
  cat >"$_package_dir/agent-harness" <<'SH'
#!/bin/bash
set -euo pipefail

case "${1:-}" in
  --version)
    echo "agent-harness 9.9.9"
    ;;
  install | verify)
    printf '%s\n' "$*" >>"${AGENT_HARNESS_TEST_LOG:?}"
    ;;
  *)
    echo "unexpected command: $*" >&2
    exit 1
    ;;
esac
SH
  chmod +x "$_package_dir/agent-harness"
  tar -C "$BATS_TEST_TMPDIR/package" -czf "$ARCHIVE" agent-harness-x86_64-unknown-linux-musl
}

make_fake_curl() {
  cat >"$FAKE_BIN/curl" <<'SH'
#!/bin/bash
set -euo pipefail

url="${@: -1}"
printf '%s\n' "$url" >>"${URL_LOG:?}"
cat "${FAKE_ARCHIVE:?}"
SH
  chmod +x "$FAKE_BIN/curl"
}
