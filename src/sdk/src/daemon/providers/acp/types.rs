//! Private state retained while folding an ACP session stream.

use serde_json::Value;

/// Provider metadata accumulated across ACP's initial call and later patches.
#[derive(Default)]
pub(super) struct AcpToolCall {
    /// Human-facing title supplied by the provider.
    pub(super) title: String,
    /// Provider tool kind used to classify the call.
    pub(super) kind: String,
    /// Most recent structured input for the call.
    pub(super) input: Value,
}
