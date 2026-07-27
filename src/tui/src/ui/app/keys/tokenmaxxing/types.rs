//! Result type for TokenMaxxing-specific keyboard handling.
#[allow(unused_imports)]
use super::*;

/// Whether the TokenMaxxing pages consumed a key and any command they emitted.
pub(in super::super) enum TokenMaxxingKey {
    /// The key belonged to the TokenMaxxing navigation surface.
    Handled(Option<Cmd>),
    /// The key should continue to global bindings.
    Unhandled,
}
