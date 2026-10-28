//! btop, compiled in.
//!
//! The vendored btop sources (`vendor/btop`) are built into this crate by
//! `build.rs` and run on their own thread inside the host process. btop draws
//! exactly as it would on a terminal, but its terminal is virtual: the host
//! reports a size, feeds key and mouse bytes, and drains the ANSI output into
//! its own emulator. Medulla's patches to btop narrow its process box to coding
//! agents, which the host classifies and hands over with [`Btop::set_agents`].
//!
//! btop keeps its state in C++ process globals, so there is one btop per
//! process: [`Btop::start`] succeeds once, and every later call fails with
//! [`StartError::AlreadyStarted`].
//!
//! - [`handle`] — [`Btop`], the safe handle over the C ABI.
//! - [`types`] — agent tags and errors.
//! - `ffi` — the raw C ABI declared in `vendor/btop/include/medulla_btop.h`.

#[cfg(not(medulla_btop_unsupported))]
mod ffi;
pub mod handle;
pub mod types;

pub use handle::Btop;
pub use types::{AgentTag, StartError};

/// Whether this build carries btop. False on targets btop has no collector
/// for (Windows), where [`Btop::start`] always fails with
/// [`StartError::Unsupported`].
pub const SUPPORTED: bool = cfg!(not(medulla_btop_unsupported));

#[cfg(test)]
mod tests;
