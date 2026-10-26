//! Data types for the agent launch registry: registrations, guards, and handles.

use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicU64, Arc, Mutex, Weak},
};

pub(super) type ProcessIdentity = (u32, u64);
pub(super) type RegistryEntries = BTreeMap<ProcessIdentity, (u64, AgentRegistration)>;

/// Ownership metadata for a process launched to serve an agent run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRegistration {
    /// Operating-system process identifier.
    pub pid: u32,
    /// Process start time in seconds since the Unix epoch.
    pub started_at_secs: u64,
    /// Task served by the process, when it was launched for a task.
    pub task_id: Option<String>,
    /// Session served by the process, when it is session-backed.
    pub session_id: Option<String>,
    /// Provider name (for example `claude`, `codex`, or `opencode`).
    pub provider: String,
}

/// Registry of child processes whose ownership is known to Medulla.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    pub(super) entries: Arc<Mutex<RegistryEntries>>,
    pub(super) next_generation: Arc<AtomicU64>,
}

/// Removes one process identity from its registry when its owner is dropped.
#[derive(Debug)]
pub struct RegistrationGuard {
    pub(super) entries: Weak<Mutex<RegistryEntries>>,
    pub(super) key: ProcessIdentity,
    pub(super) generation: u64,
}

/// Identity-safe metadata updater for a live process registration.
#[derive(Debug, Clone)]
pub struct RegistrationHandle {
    pub(super) entries: Weak<Mutex<RegistryEntries>>,
    pub(super) key: ProcessIdentity,
    pub(super) generation: u64,
}
