//! Whole-device CPU, memory, and disk-capacity formatting for the sidebar.
//!
//! Where the sibling process monitor answers "what is Medulla costing this
//! machine?", this module answers "how much room does the machine have left?".
//! Every reading is optional: a platform that cannot report one metric still
//! renders the others.

mod format;
#[cfg(test)]
mod tests;

pub use format::{device_lines, device_width_hint};
