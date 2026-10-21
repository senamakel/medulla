//! PTY screen primitives shared by the SDK and TUI.
//!
//! The `screen` module owns the `medulla.screen.v1` wire model and codec.
//! `convert` bridges that model to terminal cells, while `cell_text` keeps
//! common single-cell text inline. Process handles and managers remain in the
//! TUI crate.

pub mod cell_text;
pub mod convert;
pub mod screen;
pub mod sync;
#[cfg(test)]
mod lifecycle_tests;

pub use cell_text::CellText;
pub use convert::{ScreenCell, ScreenSnapshot};

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod conversion_tests;
