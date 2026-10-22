//! Platform-neutral coding-agent process discovery and classification.

mod classify;
mod types;
mod registry;

pub use classify::{discover_agents, discover_agents_with_options};
pub use types::{
    AgentDiscoveryOptions, AgentKind, AgentOrigin, AgentProcess, IoCounters, KnownProcessRoot,
    ProcessNode, ProcessRecord, ProcessUsage,
};
pub use registry::registered_roots;

#[cfg(test)]
mod tests;
