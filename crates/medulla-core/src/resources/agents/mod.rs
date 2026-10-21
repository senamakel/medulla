//! Platform-neutral coding-agent process discovery and classification.

mod classify;
mod types;

pub use classify::{discover_agents, discover_agents_with_options};
pub use types::{
    AgentDiscoveryOptions, AgentKind, AgentOrigin, AgentProcess, IoCounters, KnownProcessRoot,
    ProcessNode, ProcessRecord, ProcessUsage,
};

#[cfg(test)]
mod tests;
