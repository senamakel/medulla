//! Reviewed, built-in provider adapters and local integration diagnostics.
//!
//! This module is the single data-only registry for the coding CLIs Medulla
//! supports out of the box. It deliberately does not load provider rules from a
//! workspace or the network: executable names, version probes, supported
//! operations, and attention-rule labels are all compiled into the binary.

mod registry;
mod types;

#[cfg(test)]
mod tests;

pub use registry::{adapters, diagnose, diagnose_with, AdapterProbe};
pub use types::{
    AttentionRule, IntegrationCapabilities, IntegrationReport, ProbeStatus, ProviderAdapter,
};
