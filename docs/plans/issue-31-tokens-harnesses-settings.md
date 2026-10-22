# Plan: Tokens and Harnesses in Settings

## Decision and current shape

Implement **Tokens** and **Harnesses** as Settings subpages. The TUI already groups secondary and diagnostic surfaces in one two-pane Settings view; adding two rows there keeps the top-level tab bar compact and uses its existing keyboard and mouse navigation. Keep **Usage** as its existing page and preserve its legacy output.

The current navigation contract is the flat `SETTINGS_SUBPAGES` array plus `SETTINGS_GROUPS` in `crates/medulla-tui/src/ui/app/types/model.rs`. `crates/medulla-tui/src/ui/app/render/settings/mod.rs` dispatches the selected index, and `crates/medulla-tui/src/ui/app/keys/settings/mod.rs` routes page-specific keys. `crates/medulla-tui/src/ui/app/render/settings/usage.rs` renders Usage from the runtime event snapshot and account payload. Render tests use Ratatui `TestBackend`; tests can select a settings index and call the settings renderer with fixed state.

The primitives now live in `medulla-tokens`, `medulla-providers`, and `medulla-harness`. The Tokens surface should consume usage and pricing APIs from `medulla-tokens`; the Harnesses surface should display provider detection/routing/budget facts from `medulla-providers` and applied customizations from `medulla-harness`. Keep collection separate from rendering so tests inject deterministic view data and never depend on installed CLIs, the host environment, network calls, or live subscription probes.

## TDD implementation steps

Each step is a small red-green-refactor change. Add the assertion first, run the focused test to confirm it fails for the intended missing behavior, implement only that behavior, and rerun the focused test before moving on.

### 1. Register Settings sections

**Files**

- Change `crates/medulla-tui/src/ui/app/types/model.rs`.
- Change `crates/medulla-tui/src/ui/app/render/settings/mod.rs`.
- Change `crates/medulla-tui/src/ui/app/keys/settings/mod.rs` only if a new page-specific key is needed; otherwise rely on the existing generic navigation.
- Add or extend `crates/medulla-tui/src/ui/app/render/settings/tests.rs` for navigation/render dispatch assertions.

**Red:** Assert `SETTINGS_SUBPAGES` contains `Tokens` and `Harnesses` in the intended GENERAL section, `SETTINGS_GROUPS` remains ordered, keyboard selection reaches both new indices, and `draw_settings` renders each page title when selected. These should fail before registering the pages.

**Green:** Add stable page indices and route both indices from the Settings renderer. Keep Usage at its existing index unless the test shows that insertion is required; if indices move, update all `SP_*` constants and their match arms together. Verify existing numeric shortcuts and mouse selection still select the matching row.

### 2. Add an injected Tokens view and its TestBackend coverage

**Files**

- Add `crates/medulla-tui/src/ui/app/render/settings/tokens.rs`.
- Add `crates/medulla-tui/src/ui/app/render/settings/tokens_tests.rs` (or a `tests.rs` child module if that better matches the final module layout).
- Change `crates/medulla-tui/src/ui/app/render/settings/mod.rs` to declare and dispatch the page.
- Change `crates/medulla-tui/Cargo.toml` and `Cargo.lock` only if direct primitive-crate dependencies are needed; keep crate layering rules intact.
- Change `crates/medulla-tui/src/ui/app/types/model.rs` or a focused sibling type file only if App needs to own the injected Tokens view model.

**Red:** Render a fixed usage fixture with Ratatui `TestBackend` and assert the visible output includes separate input, output, cache-read, cache-write, and reasoning values, a cost when `medulla-tokens` can price the provider/model, and subscription meter values supplied by the fixture. Include an unknown/unpriced model case and assert cost is displayed as unavailable rather than `$0` or a guessed amount. The test must not invoke provider detection or subscription network sampling.

**Green:** Introduce the smallest view-data boundary needed to render supplied session/provider usage and subscription meters. Derive cost through `medulla_tokens::cost`; preserve unknown values as unavailable. Aggregate or format in a pure helper where useful, and keep the Ratatui code in `medulla-tui`.

### 3. Add an injected Harnesses view and its TestBackend coverage

**Files**

- Add `crates/medulla-tui/src/ui/app/render/settings/harnesses.rs`.
- Add `crates/medulla-tui/src/ui/app/render/settings/harnesses_tests.rs` (or a `tests.rs` child module if that better matches the final module layout).
- Change `crates/medulla-tui/src/ui/app/render/settings/mod.rs` to declare and dispatch the page.
- Change `crates/medulla-tui/Cargo.toml` and `Cargo.lock` only if direct primitive-crate dependencies are needed.
- Change a focused TUI view-data type file or `crates/medulla-tui/src/ui/app/types/model.rs` only if App must retain injected provider/harness facts.

**Red:** Supply a fixed fixture containing at least two detected providers with distinct versions, routing and budget values, plus representative Medulla customizations (hooks, Codex overrides, attribution, MCP/skills attachment). Render with `TestBackend`; assert provider names, versions, the supplied routing/budget state, and customization labels appear. Include an empty-detection case. Do not call `PATH`, launch a harness, mutate user configuration, or inspect live machine state in render tests.

**Green:** Define a rendering input that is assembled outside the page from the available `medulla-providers` and `medulla-harness` APIs/configuration. Render the supplied facts without performing discovery or side effects during a draw. Show unknown/unconfigured fields honestly instead of inferring values.

### 4. Pin legacy Usage output

**Files**

- Add or extend `crates/medulla-tui/src/ui/app/render/settings/tests.rs` (or `usage_tests.rs` if the tests are split by page).
- Change `crates/medulla-tui/src/ui/app/render/settings/usage.rs` only if the regression test exposes a compatibility issue.

**Red:** Build the existing Usage fixture with input/output-only token events (no cache or reasoning fields), render the Usage subpage with `TestBackend`, and assert that the existing session tier/call totals and the current no-extra-fields presentation remain present. The test should fail if the new usage schema makes old events unreadable, panics, or replaces the established Usage content.

**Green:** Keep the Usage view backward-compatible for absent optional categories. Do not require old serialized events or fixtures to grow fields just to render the existing page.

### 5. Verify, capture, and report screenshots

Run the focused Settings test filter first, then the issue gates from the epic brief: formatting, clippy, `cargo test --locked -p medulla-tui`, full `cargo test --locked`, slim `cargo check --locked -p medulla --no-default-features`, and `scripts/check-crate-layering.sh`. Add any compile fixes to the same implementation PR and rerun affected gates.

Launch the built TUI in tmux with deterministic demo/injected data and capture **two terminal screenshots** showing the Tokens and Harnesses Settings sections. Attach both images to the PR description (one per view); include enough of the navigation and page body to establish these are Settings sections. Screenshots supplement, and do not replace, the injected `TestBackend` assertions.

## Acceptance checklist

- [ ] Tokens and Harnesses appear as selectable Settings sections; top-level tabs are unchanged.
- [ ] Tokens shows per-session and per-provider input, output, cache-read, cache-write, reasoning, known cost, and subscription meters; unknown prices remain explicitly unknown.
- [ ] Harnesses shows detected providers, versions, routing, budgets, and applicable Medulla customizations from supplied data.
- [ ] Both views have deterministic `TestBackend` tests using injected data.
- [ ] A regression test proves input/output-only legacy Usage still renders without cache/reasoning fields.
- [ ] Two tmux screenshots are attached to the ready PR.
- [ ] Focused and full issue gates pass; changed Rust files remain within the repository's 500-line limit; no `vendor/` changes.
