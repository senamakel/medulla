//! Process ownership for agent launches managed by Medulla.

mod registry;

pub use registry::{
    agent_registry, current_process_identity, AgentRegistration, AgentRegistry, RegistrationGuard,
    RegistrationHandle,
};

#[cfg(test)]
mod registry_tests;
