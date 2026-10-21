//! Platform-neutral coding-agent process discovery and classification.

mod classify;
mod types;

pub use classify::discover_agents;
pub use types::{
    AgentKind, AgentOrigin, AgentProcess, IoCounters, KnownProcessRoot, ProcessNode, ProcessRecord,
    ProcessUsage,
};

#[cfg(test)]
mod tests;
