use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

use sysinfo::{Pid, ProcessesToUpdate, System};

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
    entries: Arc<Mutex<BTreeMap<(u32, u64), (u64, AgentRegistration)>>>,
    next_generation: Arc<AtomicU64>,
}

impl AgentRegistry {
    /// Register a process using its current operating-system start time.
    ///
    /// If the process cannot be found, no guard is returned and the registry is
    /// left untouched. This avoids creating an ownership claim that cannot be
    /// distinguished from a later reuse of the PID.
    pub fn register(
        &self,
        pid: u32,
        task_id: Option<String>,
        session_id: Option<String>,
        provider: impl Into<String>,
    ) -> Option<RegistrationGuard> {
        let started_at_secs = process_start_time(pid)?;
        Some(self.register_at(AgentRegistration {
            pid,
            started_at_secs,
            task_id,
            session_id,
            provider: provider.into(),
        }))
    }

    /// Register a known identity. Exposed for deterministic tests and callers
    /// that already sampled the process table at the spawn seam.
    pub fn register_at(&self, registration: AgentRegistration) -> RegistrationGuard {
        let key = (registration.pid, registration.started_at_secs);
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        lock(&self.entries).insert(key, (generation, registration));
        RegistrationGuard {
            entries: Arc::downgrade(&self.entries),
            key,
            generation,
        }
    }

    /// Return a stable snapshot of the current registrations.
    pub fn snapshot(&self) -> Vec<AgentRegistration> {
        lock(&self.entries)
            .values()
            .map(|(_, registration)| registration.clone())
            .collect()
    }

    /// Return the registration only when the sampled PID still has its
    /// registered start time. A reused PID is therefore never treated as a
    /// Medulla-owned process.
    pub fn current_for(&self, pid: u32, started_at_secs: u64) -> Option<AgentRegistration> {
        lock(&self.entries)
            .get(&(pid, started_at_secs))
            .map(|(_, registration)| registration.clone())
    }
}

/// Removes one process identity from its registry when its owner is dropped.
#[derive(Debug)]
pub struct RegistrationGuard {
    entries: Weak<Mutex<BTreeMap<(u32, u64), (u64, AgentRegistration)>>>,
    key: (u32, u64),
    generation: u64,
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        if let Some(entries) = self.entries.upgrade() {
            let mut entries = lock(&entries);
            if entries
                .get(&self.key)
                .is_some_and(|(generation, _)| *generation == self.generation)
            {
                entries.remove(&self.key);
            }
        }
    }
}

/// The process-wide registry used by launch paths and resource discovery.
pub fn agent_registry() -> &'static AgentRegistry {
    static REGISTRY: OnceLock<AgentRegistry> = OnceLock::new();
    REGISTRY.get_or_init(AgentRegistry::default)
}

fn process_start_time(pid: u32) -> Option<u64> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(sysinfo::Process::start_time)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
