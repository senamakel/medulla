# Developing

Developer documentation for Medulla lives in the Developers section of the
GitBook docs — authored in the
[tinyhumansai/medulla](https://github.com/tinyhumansai/medulla) documentation
repository, so there is a single source of truth for build, run, and
architecture detail:

- [Developers overview](https://tinyhumans.gitbook.io/medulla/developers)
- [Getting Started](https://tinyhumans.gitbook.io/medulla/developers/getting-started) covers install, building from source, and the first run.
- [CLI Reference](https://tinyhumans.gitbook.io/medulla/developers/cli-reference) covers the TUI, `medulla daemon`, the `claude`/`codex`/`opencode` wrappers, and `medulla update`.
- [Configuration](https://tinyhumans.gitbook.io/medulla/developers/configuration) covers the Medulla home directory, layered config, and the two runtimes (the backend, and the offline mock).
- [Authentication](https://tinyhumans.gitbook.io/medulla/developers/authentication) covers `medulla login`, tokens, and the loopback security model.
- [Architecture](https://tinyhumans.gitbook.io/medulla/developers/architecture) covers the crate split, the `Runtime` trait, the in-process agent loop, the workflow plane, and the host-link bridge.
- [Contributing](https://tinyhumans.gitbook.io/medulla/developers/contributing) covers build, test, lint, coverage, and releasing.

The engineering specs this code refers to — `docs/host-link-protocol.md`,
`docs/workflows.md`, `docs/vendoring.md` and the rest — live in that same
repository under
[`docs/`](https://github.com/tinyhumansai/medulla/tree/main/docs). A bare
`docs/…` path in a source comment means a file there, not one here.

## Quick start

```sh
make init                       # submodules, rustfmt/clippy, locked deps, pre-push hook
cargo run                       # debug build, starts the TUI (login screen if signed out)
cargo test                      # unit + feature + e2e suites (all mocked, no network)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Building needs a C++23 compiler as well as Rust: the Resources tab is btop,
compiled into the binary from `vendor/btop` by `src/btop/build.rs`. Use GCC 14+
or Clang 18+ on Linux (on Ubuntu 24.04, `apt install g++-14` — the build picks
it up when the default `g++` is older) and Xcode 16+ on macOS. Windows builds
without it, and the tab says btop is unavailable there.

Run tests, Clippy, and formatting before pushing. See
[Contributing](https://tinyhumans.gitbook.io/medulla/developers/contributing)
for the full loop and the release process.

## Releasing

`Release` (`.github/workflows/release.yml`, dispatched manually on `main`)
takes a `release_type` of `patch`, `minor`, or `major`. It bumps the shared
`[workspace.package]` version and the workspace entries in `Cargo.lock` with
`scripts/release/bump-version.sh`, commits `chore(release): vX.Y.Z` to `main`,
pushes the matching annotated tag, builds the binary for every supported target
from that tag, and publishes the packaged artifacts — binaries, checksums, and
the `latest.json` update manifest `medulla update` reads — as a GitHub Release.

```sh
gh workflow run release.yml -R tinyhumansai/medulla -f release_type=minor
```

`bash scripts/release/test-bump-version.sh` exercises the bump script offline;
`bash scripts/release/bump-version.sh minor` shows the bump on your own tree
(revert `Cargo.toml` and `Cargo.lock` afterwards).
