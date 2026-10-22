//! The Status line settings page model and its persistence glue.

mod logic;
mod types;

#[cfg(test)]
mod tests;

pub(super) use logic::{STATUS_LINE_ROW_COUNT, STATUS_LINE_ROWS, status_line_group};
pub(super) use types::*;
