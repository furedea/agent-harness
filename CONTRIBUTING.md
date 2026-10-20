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

Run the Rust and Nix gates:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
nix flake check
```

Run the Codex policy conformance test after installing Bats and `jq`. It also requires Node.js and
the Codex CLI from the locked package:

```bash
npm ci --ignore-scripts
CODEX_BIN="$PWD/node_modules/.bin/codex" \
  bats --print-output-on-failure tests/codex/execpolicy.bats
```

CI additionally runs Nix linting, dependency audits, GitHub Actions linting, dependency review, and
CodeQL.

## Release

Releases are managed by Release Please. Merging the release PR updates `Cargo.toml`, `Cargo.lock`,
`.release-please-manifest.json`, and `CHANGELOG.md`, tags the merge commit, and creates a draft
GitHub Release. The `Release Please` workflow then starts the `Release Artifacts` workflow on that
tag, which tests and builds the `x86_64-unknown-linux-musl` cargo-dist archive, smoke-tests it,
attests its provenance, and uploads the archive, its checksum, and a shell installer. The release is
published only after every step succeeds; if one fails, it stays a draft.

To resume a draft release whose artifact run failed, start the same workflow on the tag so the
attestation records the tagged commit:

```bash
tag=agent-harness-v0.9.5
gh workflow run release_artifacts.yml --ref "$tag" -f tag_name="$tag"
```

The workflow fails before building when it is started from any other ref or when the release is
already published. A published release is never rebuilt in place; ship a fix as a new release.
