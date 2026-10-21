//! PTY screen primitives shared by the SDK and TUI.
//!
//! The `screen` module owns the `medulla.screen.v1` wire model and codec.
//! `convert` bridges that model to terminal cells, while `cell_text` keeps
//! common single-cell text inline. Process handles and managers remain in the
//! TUI crate.

pub mod cell_text;
pub mod clipboard;
pub mod attention;
pub mod convert;
pub mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
pub mod screen;
pub mod sync;

pub use cell_text::CellText;
pub use clipboard::ClipboardSink;
pub use attention::{AttentionKind, HarnessAttention};
pub use convert::{ScreenCell, ScreenSnapshot};
pub use lifecycle::{LifecycleSession, SessionClose, SessionLifecycle};

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod conversion_tests;
