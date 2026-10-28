//! The Resources tab's content: btop itself, compiled in and drawn natively.
//!
//! [`medulla_btop`] runs the vendored btop on its own thread with a virtual
//! terminal. This module is the host side of that terminal: a `vt100` emulator
//! that btop's output is drained into, a blit of that emulator's grid into the
//! ratatui frame, the encoding of crossterm keys and mouse events into the
//! bytes btop expects, and the feed that tells btop which processes are coding
//! agents.
//!
//! btop holds its state in process globals and starts at most once per process,
//! so there is exactly one [`BtopPane`], behind [`pane`], shared by every
//! [`App`](crate::ui::app::App) the process builds.
//!
//! - [`types`] — [`BtopPane`] and its lifecycle state.
//! - [`pane`] — starting, showing, hiding and draining btop.
//! - [`render`] — the emulator grid to ratatui cells.
//! - [`input`] — crossterm events to terminal bytes.
//! - [`feed`] — the background agent classification btop's agents box shows.

use std::sync::{Mutex, MutexGuard, OnceLock};

pub mod feed;
pub mod input;
mod pane;
pub mod render;
pub mod types;

pub use types::BtopPane;

#[cfg(test)]
mod tests;

/// The process's one btop pane. A poisoned lock is recovered rather than
/// propagated: the pane holds only a view of btop, never invariants a
/// panicking render could leave half-updated.
pub fn pane() -> MutexGuard<'static, BtopPane> {
    static PANE: OnceLock<Mutex<BtopPane>> = OnceLock::new();
    PANE.get_or_init(|| Mutex::new(BtopPane::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
