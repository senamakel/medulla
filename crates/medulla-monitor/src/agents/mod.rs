//! Process ownership for agent launches managed by Medulla.

mod registry;

pub use registry::{agent_registry, AgentRegistration, AgentRegistry, RegistrationGuard};

#[cfg(test)]
mod registry_tests;
