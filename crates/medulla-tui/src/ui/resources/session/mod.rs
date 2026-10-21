//! Per-session CPU, memory, and disk-I/O formatting for harness pane titles.
//!
//! Where the sibling process monitor answers "what is Medulla costing this
//! machine?" and [`device`](super::device) answers "how much room is left?",
//! this module answers the question an operator actually has in front of a
//! pane: *what is this agent costing me right now?* — summed over the harness's
//! whole process tree, because the compiler it forked is the part that costs.
//!
//! Sampling is shared across every session by [`medulla_monitor`].

mod format;
#[cfg(test)]
mod tests;

pub use format::{session_segments, session_usage};
