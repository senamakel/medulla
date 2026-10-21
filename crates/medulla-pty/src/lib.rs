//! PTY screen primitives shared by the SDK and TUI.
//!
//! The `screen` module owns the `medulla.screen.v1` wire model and codec.
//! `convert` bridges that model to terminal cells, while `cell_text` keeps
//! common single-cell text inline. Process handles and managers remain in the
//! TUI crate.

extern crate self as medulla_pty;

pub mod attention;
pub mod cell_text;
pub mod clipboard;
pub mod convert;
pub mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
pub mod screen;
pub mod sync;
pub mod worker;

pub use attention::{AttentionKind, HarnessAttention};
pub use cell_text::CellText;
pub use clipboard::ClipboardSink;
pub use medulla_types::Checkout;
pub use convert::{ScreenCell, ScreenSnapshot};
pub use lifecycle::{AttentionStatus, LaunchCheckout, LifecycleSession, OpaqueOrigin, SessionClose, SessionLifecycle};

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod conversion_tests;
