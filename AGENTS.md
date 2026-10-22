# Repository Guidelines

## What this repository contains

This is the public Medulla repository. It contains the Rust client workspace,
its vendored dependency submodules, tests and e2e harnesses, documentation,
GitBook sources, installers, and release workflows. The hosted orchestration
service is operated separately; do not add service-side implementation here.

- `crates/` — the workspace crates: shared types, provider registry, harness
  customization, monitor, link, compressor, PTY, token accounting, the `medulla`
  SDK, and `medulla-tui`. See [Architecture](gitbooks/developers/architecture.md)
  for the crate map and dependency direction.
- `vendor/` — pinned source submodules used by the Rust workspace.
- `docs/` and `gitbooks/` — engineering specs and published documentation.
- `.github/`, `scripts/`, `e2e/`, and `examples/` — CI/release automation,
  development helpers, integration suites, and examples.

## Working rules

- Make changes on a feature branch in a worktree. Run `worktree <slug>` before
  editing and work inside the reported `./worktrees/<slug>` path.
- Keep source, tests, docs, and workflows together when a change affects them.
- Preserve vendored dependency pins unless deliberately updating a dependency.
- Keep CI and release workflows usable with standard GitHub Actions permissions;
  never require organization-only secrets for routine builds or releases.
- Never commit secrets, `.env` files, or machine-local configuration.

## Checks

Initialize dependencies and developer tooling with `make init`. The primary
checks are:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --all-targets
```

Keep product primitives as leaves: crates under `crates/` must not depend on
`medulla` or `medulla-tui`, and only `medulla-tui` may have normal `ratatui` or
`crossterm` dependencies. Run `scripts/check-crate-layering.sh` to validate
these boundaries; justified exceptions belong in `scripts/crate-layering.allow`
and must name both the crate and dependency. The fixture regression check is
`scripts/tests/layering/run.sh`.

Use `make ci` to run the full local gate. See [DEVELOPING.md](DEVELOPING.md)
and [Contributing](gitbooks/developers/contributing.md) for the development
workflow.
