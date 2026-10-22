use super::{AgentOrigin, KnownProcessRoot};

/// Snapshot currently registered launch roots for process discovery.
///
/// A registration is admitted only when both PID and sampled start time match,
/// so a recycled PID cannot carry its previous task or session ownership.
pub fn registered_roots() -> Vec<KnownProcessRoot> {
    medulla_monitor::agents::agent_registry()
        .snapshot()
        .into_iter()
        .map(|registration| KnownProcessRoot {
            pid: registration.pid,
            started_at_secs: registration.started_at_secs,
            origin: match (registration.task_id, registration.session_id) {
                (Some(id), _) => AgentOrigin::Task { id },
                (None, Some(id)) => AgentOrigin::Session { id },
                (None, None) => AgentOrigin::Headless,
            },
        })
        .collect()
}
