# Issue 33: TinyJuice compression through the embedded runtime

Issue: `senamakel/medulla#33`, part of `tinyhumansai/medulla#18`.
Prerequisites: #21 and #31 are on the epic branch.
Implementation checkout: `/home/ubuntu/work/workflow-medulla/worktrees/epic-18-33-compression/medulla`.

This is an implementation plan, not an implementation. Only this document was
changed during planning. Follow the active user's canonical-upstream PR rules
when they differ from the epic skill's fork PR examples. Do not edit vendored
source, change gitlinks, or introduce another TinyJuice package source.

## Decision and evidence

Choose **route (a)**: enable OpenHuman's existing tool-result middleware for
embedded turns, and read its persisted savings through `medulla-juice`.

The flag alone is insufficient. The root dependency currently enables only
`openhuman-embed`'s `skills` and `storage-sqlite` features. In
`vendor/openhuman/crates/openhuman-core/src/inference/tokenjuice/mod.rs`,
`proxy()` returns "native modules are not compiled into this build" without
the `modules` feature. With it enabled, the host lazily loads the pinned
TinyJuice module and calls `Install` and `CompactWith`, with a legacy `Compact`
fallback. Configuration/load/call errors return the original text.

`agent/session_host/runtime_session.rs:837` copies
`context.compaction_enabled()` into `TurnContextMiddleware`. Its tool-output
middleware replaces the model-visible result before history ingestion, and
the builder advertises the recovery tools when compaction is on. The middleware
already protects exact file reads, recovery calls, and raw fetches. Preserve
these rules; do not try to replace results through Medulla's `ToolHook`.

The module registry pins release 0.6.1, including per-platform archive digests.
Both OpenHuman's source dependency and Medulla's new optional dependency must
resolve to `vendor/openhuman/vendor/tinyjuice`, version 0.6.1. Read-only use of
this existing path is allowed; editing vendor source is not.

The research checkout `/home/ubuntu/work/libraries/tinyjuice` is 0.11.1,
edition 2024, Rust 1.88, GPL-3.0-only. It has **no root CHANGELOG**. Inspecting
its manifest, git history, and exported API instead shows a new
`tinyjuice-drivers` workspace member, a pinned storage-schema dependency,
module/storage migration work, and removal of the public `ml` module. The
public compressor/typed-transform/SDK exports needed here already exist at
Medulla's 0.6.1 pin. Upgrading would add a separate OpenHuman migration and
module-registry release update without being required for this issue.

**Claude/Codex scope decision:** keep the built-in PostToolUse shim silent on
stdout. `crates/medulla-tui/src/commands/hook.rs` explicitly guarantees exit
zero, silent stdout, bounded work, and summary-only reporting. It is not a
result-replacement API. This decision is about Medulla's current integration,
not a claim that every version of either harness lacks replacement APIs.
Compression in this issue applies to embedded OpenHuman turns only.

## Public contracts and feature behavior

Keep `CompressInput`, `Compressed`, `CompressionStats`, `Compressor`, and
`Passthrough` source compatible. In particular, the trait remains synchronous
and byte fields keep their existing `usize` type and exact-byte meaning.

Add these interfaces to `medulla-juice`:

```rust
#[cfg(feature = "tinyjuice")]
#[derive(Clone, Copy, Debug, Default)]
pub struct TinyJuice;

pub fn compress(input: &CompressInput<'_>) -> Compressed;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompressionTokenTotals {
    pub before_tokens: u64,
    pub after_tokens: u64,
}

impl CompressionTokenTotals {
    pub fn saved_tokens(self) -> u64;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedCompressionStats {
    pub events: u64,
    pub tokens: CompressionTokenTotals,
}

pub fn read_embedded_stats(workspace: &std::path::Path)
    -> std::io::Result<Option<EmbeddedCompressionStats>>;
```

`compress()` dispatches to `TinyJuice` with the feature and `Passthrough`
without it. It never updates embedded-runtime totals: those describe work
actually performed inside the runtime. Add `CompressionStats::token_totals()`
to expose the same token view without changing its fields.

The synchronous `TinyJuice` implementation uses TinyJuice's existing
`compressors::json::SmartCrusherTableTransform` and `ReformatTransform`.
Construct `PipelineInput { content, original_content: content,
content_kind: ContentKind::Json, original_bytes: content.len() }`; call `apply`.
This transform checks JSON, declines lossy row dropping, and performs faithful
tabular reformatting. Accept an output only when both its UTF-8 byte length
and estimated token count are no greater than the input and at least one is
smaller. Otherwise return `Passthrough`. Use the existing Unicode-char
`div_ceil(4)` estimate for both counts. Do not block on the asynchronous router,
create Tokio runtimes, or emit unavailable CCR retrieval handles from this
standalone API. Its documented scope is faithful JSON reformatting; the
embedded module owns the broader content router and recovery cache.

Add manifest features exactly as follows, preserving existing defaults:

```toml
# crates/medulla-juice/Cargo.toml
[features]
default = []
tinyjuice = ["dep:tinyjuice"]

[dependencies]
tinyjuice = { path = "../../vendor/openhuman/vendor/tinyjuice", optional = true, default-features = false }
serde = { workspace = true }
serde_json = { workspace = true }

# crates/medulla-core/Cargo.toml additions
# [features]
tinyjuice = ["medulla-juice/tinyjuice", "openhuman-embed/modules"]
# [dependencies]
medulla-juice = { workspace = true }

# crates/medulla-tui/Cargo.toml [features] addition
tinyjuice = ["medulla/tinyjuice"]
```

Add `pub use medulla_juice as juice;` to the SDK root. Primitive crates must
not depend on the SDK, OpenHuman, ratatui, or crossterm. Default-off makes
`--no-default-features` and the existing release behavior unchanged; explicitly
built `--features tinyjuice` artifacts opt into module loading and compression.
Do not silently add the feature to release workflows in this issue.

## Task 1: implement the primitive with red/green fixtures

Files:

- Modify `crates/medulla-juice/Cargo.toml`, `src/lib.rs`.
- Add `crates/medulla-juice/src/tinyjuice.rs`.
- Add `crates/medulla-juice/tests/compression.rs` and
  `crates/medulla-juice/tests/fixtures/tool-output.json`.
- Modify root `Cargo.lock` only through Cargo resolution; no vendor lockfiles.

1. Add a fixture of 12 JSON objects with repeated long key names, distinct
   scalar values, and one non-ASCII string. Keep it below TinyJuice's 40-row
   lossy threshold. Test `compress()` against it with the feature enabled.
   First run must fail because the API/implementation is absent or passthrough.
2. Implement the adapter described above. Assert shorter output, all fixture
   values retained, `before_bytes == fixture.len()`,
   `after_bytes == result.text.len()`, and both token estimates independently
   calculated from Unicode scalar counts. Do not pin the full table's spacing.
3. Add passing edge cases for empty, whitespace-only, malformed JSON, plain
   text, unsupported JSON, and a >40-row fixture whose transform would drop
   rows. All must preserve the original when the transform declines.
4. With `#[cfg(not(feature = "tinyjuice"))]`, assert the same fixture passed to
   the facade is byte-for-byte unchanged with equal before/after stats.
5. Run `cargo test --locked -p medulla-juice` and the same command with
   `--features tinyjuice`. Record the initial failing and final passing commands.

## Task 2: read middleware totals without inventing measurements

Files:

- Add `crates/medulla-juice/src/embedded.rs`.
- Add `crates/medulla-juice/tests/embedded_stats.rs`.
- Modify `crates/medulla-juice/src/lib.rs` to export the types/functions above.

The source is `<workspace>/state/tokenjuice_savings.json`, written by
OpenHuman's `inference/tokenjuice/savings.rs`. Its relevant shape is:

```json
{"total":{"events":2,"originalTokens":1200,"compactedTokens":300,
"tokensSaved":900,"costSavedUsd":0.01},"byModel":{},"byCompressor":{}}
```

1. First write fixtures for valid totals, absent file, malformed/truncated JSON,
   unknown fields, oversized file, and invalid/inverted counts. Run them red.
2. Read at most 1 MiB plus one byte via `Read::take`; reject oversized and
   invalid content with `InvalidData`. Return `Ok(None)` only for NotFound.
   Deserialize just required `total.events`, `originalTokens`, and
   `compactedTokens`; ignore future fields. Reject `compactedTokens >
   originalTokens`. `saved_tokens()` uses saturating subtraction.
3. Never reinterpret tokens as bytes or provider-billed tokens. The native
   aggregate records successful savings only, is shared by all embedded turns
   in the workspace, and survives restarts. Do not subtract global before/after
   snapshots to attribute savings to one concurrently running session.
4. `CompressionStats` continues representing full standalone measurements;
   `EmbeddedCompressionStats` deliberately has token counts only. This is the
   honest implementation of the requested Tokens-view stats at the current
   upstream boundary. Document that byte savings and per-session savings are
   unavailable from this source.

## Task 3: activate existing embedded middleware with explicit opt-out

Files:

- Modify `crates/medulla-core/Cargo.toml`, `src/lib.rs`.
- Modify `crates/medulla-core/src/daemon/providers/embed/host.rs`.
- Add `crates/medulla-core/src/daemon/providers/embed/compression.rs` and its
  sibling `compression_tests.rs`; declare the module in `embed/mod.rs`.
- Add `crates/medulla-core/tests/feature_embed_compression.rs` and test support
  beside it if needed; keep every Rust file under 500 lines.

1. Put the runtime flag mapping in a small pure helper taking
   `&mut openhuman_embed::RuntimeConfig` and a requested boolean. Unit tests
   first: effective enabled means `cfg!(feature = "tinyjuice") && requested`;
   feature absent or requested false explicitly sets
   `config.context.compaction_enabled = false`. Do not change context budgets,
   truncation, autocompaction, tool hooks, or unrelated module flags.
2. Add `EmbedHost::with_compression_enabled(self, bool) -> Self`, storing the
   requested boolean. Replace derived `Default` with an explicit initializer
   whose request defaults to `cfg!(feature = "tinyjuice")`. Keep `with_budget`
   compatible by initializing the new field through `Self::default()`.
   Applying the setter before the first turn is the supported use; reject a
   conflicting policy when an existing process runtime is reused.
3. Apply the helper to the runtime config inside `EmbedHost::runtime` before
   `Runtime::builder().config(config)`. Record effective policy in
   `OwnedRuntime` and compare it alongside the existing account-home check.
   This avoids a second host silently borrowing a runtime with another policy.
   Feature-off true requests resolve to false, preserving passthrough.
4. Leave `run/execution.rs`'s tool hooks alone: config at runtime creation is
   the actual path feeding `runtime_session.rs`; changing an after-tool hook
   cannot replace content. No double compression of progress events.
5. Add an offline provider fixture patterned after `tests/feature_embed_agents.rs`.
   Use its configured worker stack/runtime, temporary Medulla home and cwd,
   local Wiremock completion responder, and a two-step tool-call/answer turn.
   The tool reads the JSON fixture through `shell` (`cat` on Unix;
   `Get-Content -Raw` on Windows). Keep output above module compression minimum
   and below existing result-budget caps. Inspect the **second provider request**
   to assert actual model-visible output, not only the hook/progress output.
6. Disabled regression (ordinary nonignored test): construct a host with the
   explicit opt-out, run the tool turn, and assert the exact fixture reaches
   the next model request and no savings snapshot is created. This test must
   also run under the enabled Cargo feature. Isolate process-global runtime
   state by running scenarios in separate integration-test processes when
   necessary; never race mutation of global environment variables.
7. Enabled module integration test: require `TINYJUICE_TEST_MODULE` to name an
   explicitly built cdylib, run the same turn with opt-in, assert shorter
   model-visible content, recovery-tool availability, and nonzero totals parsed
   from this temporary workspace. Use a fixture long enough to exceed the
   pinned module's configured minimum. The standalone 12-row fixture can be
   expanded with longer repeated key/value strings without crossing 40 rows.
   Mark the test ignored with a precise fixture requirement and run it
   explicitly during local verification; do not silently return success when
   the module is absent. A failed load must not count as enabled-path proof.
8. Add an unavailable-module scenario using a known nonexistent override path
   in an isolated process: model-visible output stays unchanged and the turn
   succeeds. Record this separately from the successful compression test.

OpenHuman recognizes `TINYJUICE_TEST_MODULE` in non-test dependency builds too,
so Medulla can supply the fixture without editing vendor code. Build the module
under its own checkout `target/`, never `/tmp` or `CARGO_TARGET_DIR`. Loading a
pinned released module is also possible, but must not make ordinary tests
depend on release downloads. Module asset fetching in an explicitly enabled
production build follows OpenHuman's existing pinned registry and failure
fallback policy.

## Task 4: show local embedded totals in the Tokens view

Files:

- Modify `crates/medulla-tui/Cargo.toml`.
- Modify `crates/medulla-tui/src/ui/app/render/settings/tokens.rs` and
  `view_tests.rs`.
- Modify `crates/medulla-tui/src/ui/app/types/app_state.inc.rs`,
  `types/model.rs`, and `state_constructor.inc.rs`.
- Add `crates/medulla-tui/src/ui/app/compression.rs`, declare it in `app/mod.rs`.
- Modify `crates/medulla-tui/src/event_loop/types.rs`, `mod.rs`, and
  `cmd_dispatch/mod.rs`; put new command execution in a sibling
  `cmd_dispatch/compression.rs` if the existing file would exceed 500 lines.

1. Add `compression: Option<medulla::juice::EmbeddedCompressionStats>` to
   `TokensView`. Start with renderer tests red: render 1200 before, 300 after,
   900 estimated tokens saved and 2 compression events. Also test no data.
   The row label must say **Local embedded workspace compression**, since
   these totals are neither per-session nor remote-worker totals.
2. Render token estimates and event count only. Do not display fabricated byte
   counts, cost savings, provider usage, or zero as a substitute for missing
   data. The absent state says "No embedded compression savings reported." and
   explains that Claude/Codex hooks do not compress tool output.
3. Add `Cmd::LoadCompressionStats { workspace: PathBuf }`,
   `AppMsg::CompressionStatsLoaded(Option<EmbeddedCompressionStats>)`, and app
   state for the last successful value, load-in-flight, and last refresh time.
   Follow the existing subscription command/message flow. The handler runs
   bounded file reading in `tokio::task::spawn_blocking`, using the primitive
   parser. Send a result message even on errors so loading cannot stick.
   Distinguish a missing file (clear state) from a transient invalid/partial
   write (retain the last good state); use a result enum/message if needed.
4. Resolve the workspace outside the renderer via the existing
   `medulla::home::medulla_home(&environment).join("embed/workspace")` rule.
   Never use OpenHuman's default home or a bare process-global counter.
5. Trigger refresh on the event-loop tick while the Tokens settings page is
   visible, at most once per five seconds, with one request in flight. No work
   is scheduled without the `tinyjuice` feature. Rendering only consumes the
   injected snapshot, and does not perform disk I/O or provider calls.
6. Test the reducer/throttle and result handling with injected timestamps and
   results, including a truncated in-progress write retaining the last good
   reading. Test command execution against a temporary workspace. Preserve
   existing Tokens usage/subscription rendering assertions.

## Task 5: document boundaries and run focused gates

Update `medulla-juice/src/lib.rs` crate docs: the standalone adapter performs
faithful JSON reformatting, embedded turns use the native middleware, feature
activation is explicit, and native CLI hooks remain reporting-only. Add a
short user/developer section to `DEVELOPING.md` showing
`cargo build --locked -p medulla-tui --features tinyjuice`, the module-loading
prerequisite, fail-open behavior, runtime opt-out API, and workspace-total
stats scope. Preserve the hook module's silent-stdout guarantee.

Run from the absolute worktree (include explicit `cd --` in every command):

```sh
cargo test --locked -p medulla-juice
cargo test --locked -p medulla-juice --features tinyjuice
cargo test --locked -p medulla --test feature_embed_compression
cargo test --locked -p medulla --features tinyjuice --test feature_embed_compression
cargo test --locked -p medulla-tui --features tinyjuice tokens_view
cargo test --locked -p medulla-tui --features tinyjuice compression
cargo check --locked -p medulla --no-default-features
cargo check --locked -p medulla --features tinyjuice
cargo check --locked -p medulla-tui --features tinyjuice
cargo clippy --locked -p medulla-juice -p medulla -p medulla-tui --all-targets --features tinyjuice -- -D warnings
cargo fmt --all -- --check
scripts/check-crate-layering.sh
cargo tree --locked -p medulla --features tinyjuice -i tinyjuice
cargo tree --locked -p medulla --features tinyjuice -d -i tinyjuice
```

The last duplicate command may print nothing on success; additionally inspect
`cargo metadata --locked --format-version 1 --features tinyjuice` and assert
exactly one package named `tinyjuice`, version 0.6.1 and the existing vendored
manifest path. A version match with a different package source is not enough.

Build the integration module with
`cargo build --locked --manifest-path vendor/openhuman/vendor/tinyjuice/Cargo.toml -p tinyjuice-module`.
Its normal output is under that workspace's `target/debug/` unless existing
Cargo configuration redirects it. Discover its actual cdylib path from Cargo
JSON artifacts if needed; do not assume a global target path. Run the enabled
ignored test explicitly with `TINYJUICE_TEST_MODULE=<absolute cdylib path>` and
`cargo test --locked -p medulla --features tinyjuice --test feature_embed_compression -- --ignored`.
Never count an ignored/skipped test as a successful middleware integration.

Finally run the issue's full gates once: `cargo clippy --locked --all-targets
-- -D warnings`, `cargo test --locked`, and the format/layering checks above.
Any unrelated existing failure must be reported with the command and evidence;
do not change vendor pins or unrelated code to conceal it. Windows receives
the ordinary feature/default checks and portable primitive tests; native
module integration requires the corresponding Windows cdylib fixture.

## Review checklist and completion evidence

- Single TinyJuice package identity and untouched vendor/gitlinks.
- Red/green fixture compression with exact byte and Unicode estimate assertions.
- Feature-off facade passthrough and explicit-disabled embedded-turn equivalence.
- Enabled middleware changes the next model request with a real module loaded.
- Unavailable native module preserves a successful turn and original content.
- Policy-conflicting runtime reuse fails clearly rather than applying another
  host's setting.
- Tokens rendering shows actual persisted estimates with accurate workspace
  scope, no invented bytes, and no per-turn claims from shared counters.
- Report-only Claude/Codex hook stdout stays empty.
- New or touched Rust files stay under 500 lines; SDK paths remain compatible.
- A code-reviewer checks the implementation before the PR, per the issue brief.

Do not mark #33 complete on primitive tests alone: that would leave the
embedded-turn integration and visible stats unproven.
