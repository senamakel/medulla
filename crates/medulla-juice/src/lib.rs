//! Pluggable context compression primitives for Medulla.
//!
//! This crate currently provides only a passthrough compressor and metadata
//! estimates. Token counts use TinyJuice's documented heuristic of roughly
//! four Unicode characters per token; they are estimates, not provider counts.
//!
//! Compression is not wired into either host integration yet:
//!
//! - Embedded OpenHuman's `ToolHook::after_tool` returns `()`, and
//!   `after_tool_context` can only append text. Neither can replace a tool
//!   result with compressed content (`crates/medulla-core/src/daemon/providers/
//!   embed/hooks.rs` and the vendored OpenHuman `agent/hooks.rs`).
//! - Claude and Codex use Medulla's report-only `PostToolUse` hook shim. It
//!   writes no replacement content to stdout (`crates/medulla-tui/src/commands/
//!   hook.rs`).
//!
//! These integration seams need host API changes before a compressor can
//! replace tool results. This crate deliberately has no host or compressor
//! dependencies.

/// Text passed to a [`Compressor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressInput<'a> {
    /// Original text to compress.
    pub text: &'a str,
}

impl<'a> CompressInput<'a> {
    /// Creates an input from borrowed text.
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }
}

/// Byte and approximate token counts before and after compression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompressionStats {
    /// Input size in UTF-8 bytes.
    pub before_bytes: usize,
    /// Output size in UTF-8 bytes.
    pub after_bytes: usize,
    /// Estimated input tokens, using four Unicode characters per token.
    pub before_tokens: u64,
    /// Estimated output tokens, using four Unicode characters per token.
    pub after_tokens: u64,
}

/// Compressed text and its before/after size estimates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compressed {
    /// Result text.
    pub text: String,
    /// Input and result size estimates.
    pub stats: CompressionStats,
}

/// Compresses text and reports approximate size changes.
pub trait Compressor {
    /// Compresses `input`, returning the resulting text and size estimates.
    fn compress(&self, input: &CompressInput<'_>) -> Compressed;
}

/// A no-op compressor that preserves its input exactly.
#[derive(Clone, Copy, Debug, Default)]
pub struct Passthrough;

impl Compressor for Passthrough {
    fn compress(&self, input: &CompressInput<'_>) -> Compressed {
        Compressed {
            text: input.text.to_owned(),
            stats: CompressionStats {
                before_bytes: input.text.len(),
                after_bytes: input.text.len(),
                before_tokens: estimate_tokens(input.text),
                after_tokens: estimate_tokens(input.text),
            },
        }
    }
}

fn estimate_tokens(text: &str) -> u64 {
    text.chars().count().div_ceil(4) as u64
}

#[cfg(test)]
mod tests;
