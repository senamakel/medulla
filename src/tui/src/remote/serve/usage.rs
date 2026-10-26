//! Per-session agent process usage for the session rows a host serves, so a
//! client's Resources tab can list the agents running on that host.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use medulla::protocol::{AgentUsage, RemoteSessionRow};
use medulla::resources::agents::{discover_agents, AgentOrigin, AgentProcess, KnownProcessRoot};

use crate::ui::resources::sampler::{ResourceSource, SystemResourceSource};
use crate::worker::pty::PtyManager;

/// How many CPU readings each session's usage carries.
const CPU_HISTORY: usize = 32;

/// Samples the host's processes at most once per interval and remembers each
/// session's latest usage between samples.
pub(super) struct UsageSampler {
    source: SystemResourceSource,
    sampled_at: Option<Instant>,
    interval: Duration,
    history: HashMap<(u32, u64), Vec<f64>>,
    cache: HashMap<String, AgentUsage>,
}

impl Default for UsageSampler {
    fn default() -> Self {
        Self {
            source: SystemResourceSource::default(),
            sampled_at: None,
            interval: Duration::from_millis(2_000),
            history: HashMap::new(),
            cache: HashMap::new(),
        }
    }
}

impl UsageSampler {
    /// Set the minimum time between process samples.
    pub(super) fn set_interval(&mut self, interval: Duration) {
        self.interval = interval.max(Duration::from_millis(1));
    }

    /// Attach each session's agent usage to its row, sampling when one is due.
    pub(super) fn annotate(
        &mut self,
        sessions: &PtyManager,
        mut rows: Vec<RemoteSessionRow>,
    ) -> Vec<RemoteSessionRow> {
        let now = Instant::now();
        let sample = self
            .sampled_at
            .is_none_or(|last| now.duration_since(last) >= self.interval);
        if sample {
            self.sampled_at = Some(now);
            // Rebuild from this sample so exited/reaped PIDs cannot leave
            // stale process metrics attached to a still-retained session row.
            self.cache.clear();
            let agents = self.discover(sessions, &rows);
            for row in &rows {
                if let Some(agent) = classified_session_agent(&agents, &row.id) {
                    self.record(&row.id, agent);
                }
            }
        }
        for row in &mut rows {
            row.usage = self.cache.get(&row.id).cloned();
        }
        // Keep the process history bounded to live identities only.
        let current: HashSet<_> = rows
            .iter()
            .filter_map(|row| {
                row.usage
                    .as_ref()
                    .map(|usage| (usage.pid, usage.started_at))
            })
            .collect();
        self.history
            .retain(|identity, _| current.contains(identity));
        self.cache
            .retain(|id, _| rows.iter().any(|row| &row.id == id));
        rows
    }

    fn discover(&mut self, sessions: &PtyManager, rows: &[RemoteSessionRow]) -> Vec<AgentProcess> {
        let snapshot = self.source.sample();
        let processes: Vec<_> = snapshot
            .processes
            .iter()
            .map(|process| process.to_record())
            .collect();
        let roots: Vec<_> = rows
            .iter()
            .filter_map(|row| {
                let pid = sessions.pid(&row.id)?;
                let process = processes.iter().find(|process| process.pid == pid)?;
                Some(KnownProcessRoot {
                    pid,
                    started_at_secs: process.started_at_secs,
                    origin: AgentOrigin::Session { id: row.id.clone() },
                })
            })
            .collect();
        discover_agents(
            &processes,
            &roots,
            std::process::id(),
            (medulla::clock::now_millis().max(0) as u64) / 1000,
        )
    }

    fn record(&mut self, session_id: &str, agent: &AgentProcess) {
        let Some(pid) = agent.pid else {
            return;
        };
        let history = self
            .history
            .entry((pid, agent.started_at_secs))
            .or_default();
        history.push(f64::from(agent.usage.cpu_percent));
        if history.len() > CPU_HISTORY {
            history.remove(0);
        }
        self.cache.insert(
            session_id.to_owned(),
            AgentUsage {
                kind: agent.kind.label().to_owned(),
                pid,
                cpu: f64::from(agent.usage.cpu_percent),
                rss: agent.usage.rss_bytes,
                threads: agent.usage.threads as usize,
                started_at: agent.started_at_secs,
                cpu_history: history.clone(),
            },
        );
    }
}

pub(super) fn classified_session_agent<'a>(
    agents: &'a [AgentProcess],
    session_id: &str,
) -> Option<&'a AgentProcess> {
    agents.iter().find(|agent| {
        agent.origin
            == (AgentOrigin::Session {
                id: session_id.to_owned(),
            })
    })
}
