# medulla-tui

The Medulla terminal application and medulla executable. It owns terminal rendering, input handling, CLI dispatch, and process wiring, and composes the SDK with the primitives needed by the app.

This is the top-level application crate. The SDK does not depend on it. See the [workspace architecture](../../gitbooks/developers/architecture.md) for the complete crate map.
