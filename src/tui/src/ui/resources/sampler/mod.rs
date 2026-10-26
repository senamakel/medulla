//! Background system resource sampling and immutable snapshots.

mod history;
mod types;

pub use history::History;
pub use types::{
    DiskSnapshot, NetworkSnapshot, ProcessSnapshot, ResourceHistory, ResourceSnapshot,
};

mod disks;
mod filters;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod source;
#[cfg(test)]
mod tests;

use disks::*;
use filters::*;
#[cfg(target_os = "linux")]
use linux::*;
#[cfg(test)]
use source::per_second;
pub use source::SystemResourceSource;

use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use sysinfo::Pid;
use sysinfo::{Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::watch;

/// A source of resource snapshots, injectable to test task ordering without
/// depending on the host machine.
pub trait ResourceSource: Send + 'static {
    /// Read the next immutable resource snapshot.
    fn sample(&mut self) -> ResourceSnapshot;

    /// Apply the current disk collection options when the source supports them.
    fn update_disk_options(&mut self, _options: DiskOptions) {}
}

/// Change the sampler cadence only when its value differs.
///
/// `watch::Sender::send_replace` wakes receivers even for identical values;
/// repeatedly doing that from the UI loop would keep restarting the timer and
/// could starve samples while input or runtime events arrive continuously.
pub fn update_interval(sender: &watch::Sender<Option<Duration>>, period: Option<Duration>) -> bool {
    sender.send_if_modified(|current| {
        if *current == period {
            false
        } else {
            *current = period;
            true
        }
    })
}

/// Update disk collection preferences only when they changed.
pub fn update_disk_options(sender: &watch::Sender<DiskOptions>, options: DiskOptions) -> bool {
    sender.send_if_modified(|current| {
        if *current == options {
            false
        } else {
            *current = options;
            true
        }
    })
}

/// Spawn a sampler task. Sampling runs on Tokio's blocking pool so a slow
/// process walk cannot hold up the UI/event-loop worker.
pub fn spawn_sampler<S, F>(
    source: S,
    interval: watch::Receiver<Option<Duration>>,
    history_capacity: usize,
    output: F,
) -> tokio::task::JoinHandle<()>
where
    S: ResourceSource,
    F: Fn(Arc<ResourceSnapshot>) -> bool + Send + Sync + 'static,
{
    spawn_sampler_inner(source, interval, None, history_capacity, output)
}

/// Spawn a sampler whose disk filters can be changed while it is running.
pub fn spawn_sampler_with_disk_options<S, F>(
    source: S,
    interval: watch::Receiver<Option<Duration>>,
    disk_options: watch::Receiver<DiskOptions>,
    history_capacity: usize,
    output: F,
) -> tokio::task::JoinHandle<()>
where
    S: ResourceSource,
    F: Fn(Arc<ResourceSnapshot>) -> bool + Send + Sync + 'static,
{
    spawn_sampler_inner(
        source,
        interval,
        Some(disk_options),
        history_capacity,
        output,
    )
}

fn spawn_sampler_inner<S, F>(
    mut source: S,
    mut interval: watch::Receiver<Option<Duration>>,
    mut disk_options: Option<watch::Receiver<DiskOptions>>,
    history_capacity: usize,
    output: F,
) -> tokio::task::JoinHandle<()>
where
    S: ResourceSource,
    F: Fn(Arc<ResourceSnapshot>) -> bool + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let mut history = ResourceHistory::default();
        loop {
            if let Some(options) = disk_options.as_mut() {
                source.update_disk_options(options.borrow_and_update().clone());
            }
            let current_interval = *interval.borrow();
            let period = match current_interval {
                Some(period) => period.max(Duration::from_millis(1)),
                None => {
                    if interval.changed().await.is_err() {
                        break;
                    }
                    continue;
                }
            };
            let mut timer = tokio::time::interval(period);
            timer.tick().await;
            tokio::select! {
                _ = timer.tick() => {
                    let sampled = tokio::task::spawn_blocking(move || {
                        let snapshot = source.sample();
                        (source, snapshot)
                    }).await;
                    let Ok((next_source, mut snapshot)) = sampled else { break };
                    source = next_source;
                    history.push(&snapshot, history_capacity);
                    snapshot.history = Arc::new(history.clone());
                    if !output(Arc::new(snapshot)) {
                        break;
                    }
                }
                changed = interval.changed() => if changed.is_err() { break; },
                changed = wait_for_disk_options(&mut disk_options) => if changed.is_err() { break; },
            }
        }
    })
}

async fn wait_for_disk_options(
    receiver: &mut Option<watch::Receiver<DiskOptions>>,
) -> Result<(), watch::error::RecvError> {
    match receiver {
        Some(receiver) => receiver.changed().await,
        None => std::future::pending().await,
    }
}

/// Optional disk list filtering, configured by the TUI without coupling this crate to its config type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskOptions {
    pub only_physical: bool,
    pub use_fstab: bool,
    pub filter: String,
}
