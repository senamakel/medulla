//! Ids and plain value types shared across Medulla crates.
//!
//! Types here describe data only. They perform no I/O and do not depend on
//! tokio, keeping this crate suitable as a leaf dependency throughout the
//! workspace.

mod checkout;
pub mod clock;
mod harness;
pub mod harness_contract;
mod session_origin;
mod token_usage;
mod tool_mode;

pub use checkout::Checkout;
pub use harness::{dispatchable_flavors, HarnessProvider, HarnessTransport, CODEX_SERVER_FLAVOR};
pub use session_origin::SessionOrigin;
pub use token_usage::{BudgetWindow, TokenUsage};
pub use tool_mode::{ToolMode, TOOL_MODE_ENV};

#[cfg(test)]
#[path = "harness_contract_tests.rs"]
mod harness_contract_tests;
#[cfg(test)]
mod tests;
