# Memory

Memory service: a thin, medulla-owned wrapper over tinycortex's persona memory layer (doc 06). It turns local coding-agent history into a durable, prompt-ready persona pack and exposes a small offline query surface (`status`/`search`/`directives`/`overview`) plus an LLM-backed ingest path.

## Contents

- [`env/`](./env/) — Pure environment-variable resolution for the memory (tinycortex persona) integration.
- [`mod.rs`](./mod.rs) — Memory service: a thin, medulla-owned wrapper over tinycortex's persona memory layer (doc 06). It turns local coding-agent history into a durable, prompt-ready persona pack and exposes a small offline query surface (`status`/`search`/`directives`/`overview`) plus an LLM-backed ingest path.
- [`tests.rs`](./tests.rs) — Unit tests for the memory service: status/overview rendering, the provider selection ladder, offline compile, and the report translations.
- [`types.rs`](./types.rs) — Data types for memory ingestion, search, and status reporting.

## Maintenance

Keep this index synchronized when responsibilities move. Put shared data structures in `types.rs`, focused unit tests in `tests.rs` or a sibling `_tests.rs`, and preserve the module-level Rust documentation as the API source of truth.
