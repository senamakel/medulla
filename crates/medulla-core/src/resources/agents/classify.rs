use super::types::{AgentProcess, KnownProcessRoot, ProcessRecord};

/// Classify and aggregate recognized processes from a platform-neutral sample.
pub fn discover_agents(
    _processes: &[ProcessRecord],
    _known_roots: &[KnownProcessRoot],
    _medulla_pid: u32,
    _sampled_at_secs: u64,
) -> Vec<AgentProcess> {
    Vec::new()
}
