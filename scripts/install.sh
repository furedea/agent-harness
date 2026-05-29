#!/bin/bash
# Install the latest agent-harness release archive into a user-local prefix.

set -euCo pipefail

function usage() {
  cat <<EOF >&2
Description:
    Install agent-harness from a GitHub Release archive.

Usage:
    curl -fsSL https://github.com/furedea/agent-harness/releases/latest/download/install.sh | bash

Environment:
    AGENT_HARNESS_VERSION      Release tag to install. Defaults to latest.
    AGENT_HARNESS_TARGET       Release target. Defaults to x86_64-unknown-linux-musl.
    AGENT_HARNESS_INSTALL_DIR  Install directory. Defaults to \$HOME/.local/agent-harness.
    AGENT_HARNESS_BIN_DIR      Symlink directory. Defaults to \$HOME/.local/bin.
    AGENT_HARNESS_PREFIX       Harness install prefix. Defaults to \$HOME.
    AGENT_HARNESS_DOWNLOAD_BASE Override release asset base URL.
EOF
  exit 1
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
fi

readonly REPOSITORY="furedea/agent-harness"
readonly VERSION="${AGENT_HARNESS_VERSION:-latest}"
readonly TARGET="${AGENT_HARNESS_TARGET:-x86_64-unknown-linux-musl}"
readonly ARCHIVE_NAME="agent-harness-${TARGET}.tar.gz"
readonly INSTALL_DIR="${AGENT_HARNESS_INSTALL_DIR:-$HOME/.local/agent-harness}"
readonly BIN_DIR="${AGENT_HARNESS_BIN_DIR:-$HOME/.local/bin}"
readonly PREFIX="${AGENT_HARNESS_PREFIX:-$HOME}"
CLEANUP_DIR=""

function main() {
  require_command curl
  require_command tar

  assert_safe_path "$INSTALL_DIR" "AGENT_HARNESS_INSTALL_DIR"
  assert_safe_path "$BIN_DIR" "AGENT_HARNESS_BIN_DIR"
  assert_safe_path "$PREFIX" "AGENT_HARNESS_PREFIX"

  local _tmp_dir
  _tmp_dir="$(mktemp -d)"
  CLEANUP_DIR="$_tmp_dir"
  trap cleanup EXIT

  local _staging_dir="$_tmp_dir/agent-harness"
  mkdir -p "$_staging_dir"

  curl -fsSL "$(archive_url)" | tar -xz -C "$_staging_dir" --strip-components=1
  validate_archive "$_staging_dir"

  mkdir -p "$(dirname "$INSTALL_DIR")" "$BIN_DIR"
  rm -rf "$INSTALL_DIR"
  mv "$_staging_dir" "$INSTALL_DIR"
  ln -sf "$INSTALL_DIR/agent-harness" "$BIN_DIR/agent-harness"

  "$BIN_DIR/agent-harness" install --prefix "$PREFIX"
  "$BIN_DIR/agent-harness" verify --prefix "$PREFIX"
}

function archive_url() {
  if [[ -n "${AGENT_HARNESS_DOWNLOAD_BASE:-}" ]]; then
    printf '%s/%s\n' "${AGENT_HARNESS_DOWNLOAD_BASE%/}" "$ARCHIVE_NAME"
    return
  fi

  if [[ "$VERSION" == "latest" ]]; then
    printf 'https://github.com/%s/releases/latest/download/%s\n' "$REPOSITORY" "$ARCHIVE_NAME"
    return
  fi

  printf 'https://github.com/%s/releases/download/%s/%s\n' "$REPOSITORY" "$VERSION" "$ARCHIVE_NAME"
}

function validate_archive() {
  local _staging_dir="$1"
  if [[ ! -x "$_staging_dir/agent-harness" ]]; then
    echo "archive is missing executable agent-harness binary" >&2
    exit 1
  fi
  if [[ ! -d "$_staging_dir/share" ]]; then
    echo "archive is missing share directory" >&2
    exit 1
  fi
}

function require_command() {
  local _command="$1"
  if ! command -v "$_command" >/dev/null 2>&1; then
    echo "required command not found: $_command" >&2
    exit 1
  fi
}

function assert_safe_path() {
  local _path="$1"
  local _name="$2"
  if [[ -z "$_path" || "$_path" == "/" ]]; then
    echo "$_name must not be empty or root" >&2
    exit 1
  fi
}

function cleanup() {
  if [[ -n "$CLEANUP_DIR" ]]; then
    rm -rf "$CLEANUP_DIR"
  fi
}

main "$@"
