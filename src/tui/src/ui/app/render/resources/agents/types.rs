//! Data types for the Agents table.

use crate::ui::app::resources::types::AgentIdentity;

/// One row of the Agents table: a harness process (or an aggregate of its tree) with its usage.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ui::app) struct AgentRow {
    pub host: Option<String>,
    pub pid: u32,
    pub started: u64,
    pub kind: String,
    pub origin: String,
    pub workspace: String,
    pub threads: usize,
    pub memory: u64,
    pub cpu: f64,
    pub runtime: u64,
    pub ppid: Option<u32>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub external: bool,
    pub remote: bool,
    pub depth: usize,
}

impl AgentRow {
    pub(in crate::ui::app) fn identity(&self) -> AgentIdentity {
        AgentIdentity {
            host: self.host.clone(),
            pid: self.pid,
            started: self.started,
        }
    }
}
