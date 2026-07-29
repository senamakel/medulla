# Client

HTTP/SSE client for the Medulla orchestration backend.

## Contents

- [`error/`](./error/) — Error type for the Medulla client.
- [`program/`](./program/) — Typed models shared by the public worker-roster and task-program endpoints.
- [`sse/`](./sse/) — Hand-rolled Server-Sent Events parsing and a reconnecting event stream.
- [`tests/`](./tests/) — Unit and integration tests for the Medulla client, split by surface: `decode_tests` covers envelope/error/run-result JSON decoding; `sse_tests` covers the SSE parser, dedupe cursor, and streaming; `integration_tests` covers the HTTP endpoint surface against a TCP stub.
- [`types/`](./types/) — JSON types mirroring the backend API responses.
- [`mod.rs`](./mod.rs) — HTTP/SSE client for the Medulla orchestration backend.

## Maintenance

Keep this index synchronized when responsibilities move. Put shared data structures in `types.rs`, focused unit tests in `tests.rs` or a sibling `_tests.rs`, and preserve the module-level Rust documentation as the API source of truth.
