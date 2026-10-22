//! Process ownership for agent launches managed by Medulla.

mod registry;

pub use registry::{AgentRegistration, AgentRegistry, RegistrationGuard, agent_registry};

#[cfg(test)]
mod registry_tests;
