//! Context compression primitives and embedded-runtime savings.
//!
//! The synchronous standalone adapter performs faithful JSON table
//! reformatting through the pinned TinyJuice release when `tinyjuice` is
//! explicitly enabled. It otherwise preserves input exactly. Embedded turns
//! use OpenHuman's native tool-result middleware when the same feature is
//! enabled; this includes its broader routing and recovery behavior.
//!
//! The built-in Claude and Codex CLI hooks remain report-only and preserve
//! silent stdout. Persisted embedded savings are workspace-wide estimated token
//! totals, not per-session/provider measurements, and contain no byte counts.

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

#[cfg(feature = "tinyjuice")]
mod tinyjuice;
#[cfg(feature = "tinyjuice")]
pub use tinyjuice::TinyJuice;

/// Dispatches to TinyJuice when explicitly enabled, otherwise preserving input.
pub fn compress(input: &CompressInput<'_>) -> Compressed {
    #[cfg(feature = "tinyjuice")]
    {
        TinyJuice.compress(input)
    }
    #[cfg(not(feature = "tinyjuice"))]
    {
        Passthrough.compress(input)
    }
}

impl CompressionStats {
    /// Returns approximate token totals, not provider-billed token counts.
    pub fn token_totals(self) -> CompressionTokenTotals {
        CompressionTokenTotals {
            before_tokens: self.before_tokens,
            after_tokens: self.after_tokens,
        }
    }
}

/// Approximate token totals for a standalone compression operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompressionTokenTotals {
    pub before_tokens: u64,
    pub after_tokens: u64,
}

impl CompressionTokenTotals {
    pub fn saved_tokens(self) -> u64 {
        self.before_tokens.saturating_sub(self.after_tokens)
    }
}

/// Persisted aggregate for successful embedded-runtime compression events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedCompressionStats {
    pub events: u64,
    pub tokens: CompressionTokenTotals,
}

mod embedded;
pub use embedded::read_embedded_stats;
