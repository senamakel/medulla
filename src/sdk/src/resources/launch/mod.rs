//! Process ownership for agent launches managed by Medulla.

mod registry;
mod types;

pub use registry::agent_registry;
#[cfg(unix)]
pub use registry::current_process_identity;
pub use types::{AgentRegistration, AgentRegistry, RegistrationGuard, RegistrationHandle};

#[cfg(test)]
mod registry_tests;
