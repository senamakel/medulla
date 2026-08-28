//! Sessions that run on another machine.
//!
//! Two halves that never run in the same process:
//!
//! - [`serve`] is the daemon side — it owns the real ptys and relays their
//!   screens. Reached by `medulla daemon --direct`.
//! - the client side, which dials a host and renders what comes back, lives with
//!   the rest of the UI.

pub mod serve;
