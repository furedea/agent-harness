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

## Provider registry

`src/provider/spec.rs` is the single source of truth for providers. `nix/providers.json`, which
drives the Home Manager module, is generated from it and must not be edited by hand. After you
change `PROVIDERS` or the projection in `tests/nix_provider_registry.rs`, regenerate the file:

```bash
UPDATE_PROVIDERS_JSON=1 cargo test --test nix_provider_registry
```

Without the variable the same test fails when the committed file differs from the registry.

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
GitHub Release. In the same `Release` workflow run, the `build` job tests and builds the
`x86_64-unknown-linux-musl` cargo-dist archive on that tag, smoke-tests it, and uploads the archive,
its checksum, and a shell installer as one workflow artifact. The shared `release-publish.yml` from
furedea/reusable-workflows then attests the files, uploads them to the draft, and publishes it. The
release is published only after every step succeeds; if one fails, it stays a draft.

To resume a draft release whose run failed, start the same workflow on the tag so the attestation
records the tagged commit:

```bash
tag=agent-harness-v0.9.10
gh workflow run release.yml --ref "$tag" -f tag_name="$tag"
```

The shared workflow fails before publishing when the tag does not point to the commit of the run or
when the release is already published. A published release is never rebuilt in place; ship a fix as
a new release.
