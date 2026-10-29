//! Tell btop which processes are coding agents.
//!
//! btop's agents box shows only the processes the host names. Naming them
//! takes an executable path, the full argv and a start time per process —
//! more than btop's own process list keeps — so a background thread reads
//! those with `sysinfo`, runs the SDK's agent classifier joined to Medulla's
//! launch registry, and hands btop the result. It scans only while the tab is
//! on screen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use medulla::resources::agents::{
    discover_registered_agents, AgentOrigin, AgentProcess, ProcessRecord, ProcessUsage,
};
use medulla_btop::AgentTag;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// How often the feed re-classifies while the tab is visible. Agents start and
/// stop on a human timescale; btop's own refresh is two seconds.
const PERIOD: Duration = Duration::from_secs(2);

/// Handle to the background classifier. Dropping it stops the thread.
pub struct AgentFeed {
    visible: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
}

impl AgentFeed {
    /// Start classifying, handing each result to `publish`.
    pub fn spawn(publish: impl Fn(&[AgentTag]) + Send + 'static) -> Self {
        let visible = Arc::new(AtomicBool::new(true));
        let alive = Arc::new(AtomicBool::new(true));
        let (thread_visible, thread_alive) = (Arc::clone(&visible), Arc::clone(&alive));
        let spawned = std::thread::Builder::new()
            .name("medulla-btop-agents".into())
            .spawn(move || {
                let mut system = System::new();
                while thread_alive.load(Ordering::Relaxed) {
                    if thread_visible.load(Ordering::Relaxed) {
                        publish(&scan(&mut system));
                    }
                    std::thread::sleep(PERIOD);
                }
            });
        if spawned.is_err() {
            alive.store(false, Ordering::Relaxed);
        }
        Self { visible, alive }
    }

    /// Scan only while the tab is on screen.
    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }
}

impl Drop for AgentFeed {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

/// Read the process table and classify it.
fn scan(system: &mut System) -> Vec<AgentTag> {
    // `without_tasks`: on Linux a thread is listed as a child process, which
    // would only bloat the table the classifier walks.
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .without_tasks(),
    );
    let records: Vec<ProcessRecord> = system
        .processes()
        .values()
        .map(|process| ProcessRecord {
            pid: process.pid().as_u32(),
            ppid: process.parent().map(|parent| parent.as_u32()),
            exe: process.exe().map(std::path::Path::to_path_buf),
            argv: (!process.cmd().is_empty()).then(|| {
                process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect()
            }),
            cwd: None,
            started_at_secs: process.start_time(),
            usage: ProcessUsage::default(),
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let medulla = std::process::id();
    tags(
        &discover_registered_agents(&records, medulla, now, Default::default()),
        medulla,
    )
}

/// Turn classified agents into btop's agent table.
///
/// The embedded OpenHuman runtime has no process of its own — it runs inside
/// Medulla — so it is tagged on Medulla's pid, which makes the processes it
/// spawned show under it.
pub fn tags(agents: &[AgentProcess], medulla_pid: u32) -> Vec<AgentTag> {
    agents
        .iter()
        .map(|agent| AgentTag {
            pid: agent.pid.unwrap_or(medulla_pid),
            label: agent.kind.label().to_string(),
            launched: !matches!(
                agent.origin,
                AgentOrigin::External | AgentOrigin::Remote { .. }
            ),
        })
        .collect()
}
