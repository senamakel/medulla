//! Session lifecycle UI wiring.
//!
//! The focused child modules own handoff state changes, session creation, and
//! closing a local harness. Keeping this module as wiring makes those distinct
//! controls easy to locate without coupling their implementations.

mod close;
mod handoff;
mod picker;

pub(super) use picker::is_text_input;
