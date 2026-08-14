# Contributing

## Development environment

Clone the repository and enter the Nix development shell on Apple Silicon macOS:

```bash
git clone https://github.com/furedea/agent-harness
cd agent-harness
nix develop
```

## Running local changes

The `agent-harness` command on `PATH` may refer to a previously installed Home Manager or release
build. Checking out a branch does not replace that binary. Run the current checkout through Cargo
when developing or reviewing local changes:

```bash
cargo run -- list
cargo run -- list skills
cargo run -- list hooks --provider codex
```

Use the local flake when the Nix package itself must be verified:

```bash
nix run . -- list skills
```

Render the current checkout into a target prefix by passing its source explicitly:

```bash
cargo run -- install --source "$PWD" --prefix "$HOME"
cargo run -- verify --prefix "$HOME"
```

## Quality gates

Run the Rust gates:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Run the Python skill-script gates:

```bash
uv run --python 3.14.6 --frozen ruff format --check agents/skills tests/python
uv run --python 3.14.6 --frozen ruff check agents/skills tests/python
uv run --python 3.14.6 --frozen ty check
uv run --python 3.14.6 --frozen pytest tests/python
```

Run the hook and Codex policy tests after installing their external test tools:

```bash
npm ci --ignore-scripts
bats --print-output-on-failure --recursive tests/hooks tests/install_script.bats
CODEX_BIN=codex bats --print-output-on-failure tests/codex/execpolicy.bats
```

CI additionally runs Nix linting, dependency audits, GitHub Actions linting, dependency review, and
CodeQL.

## Release

Releases are managed by Release Please. Merging the release PR updates `Cargo.toml`, `Cargo.lock`,
`.release-please-manifest.json`, and `CHANGELOG.md`, then publishes a GitHub Release with the
`x86_64-unknown-linux-musl` cargo-dist archive, its checksum, a shell installer, and build provenance
attestations.

If the release assets need to be rebuilt for an existing tag, run the `Release Please` workflow
manually with the tag name, for example:

```text
agent-harness-v0.5.0
```
