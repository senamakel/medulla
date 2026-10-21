//! Ids and plain value types shared across Medulla crates.
//!
//! Types here describe data only. They perform no I/O and do not depend on
//! tokio, keeping this crate suitable as a leaf dependency throughout the
//! workspace.

pub mod clock;
pub mod harness_contract;
mod harness;
mod token_usage;

pub use harness::{dispatchable_flavors, HarnessProvider, HarnessTransport, CODEX_SERVER_FLAVOR};
pub use token_usage::{BudgetWindow, TokenUsage};

#[cfg(test)]
mod tests;
