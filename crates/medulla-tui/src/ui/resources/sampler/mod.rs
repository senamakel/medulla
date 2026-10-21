//! Background system resource sampling and immutable snapshots.

mod history;
mod types;

pub use history::History;
pub use types::{DiskSnapshot, ProcessSnapshot, ResourceHistory, ResourceSnapshot};

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{Disks, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::sync::watch;

/// A source of resource snapshots, injectable to test task ordering without
/// depending on the host machine.
pub trait ResourceSource: Send + 'static {
    /// Read the next immutable resource snapshot.
    fn sample(&mut self) -> ResourceSnapshot;
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

/// Spawn a sampler task. Sampling runs on Tokio's blocking pool so a slow
/// process walk cannot hold up the UI/event-loop worker.
pub fn spawn_sampler<S, F>(
    mut source: S,
    mut interval: watch::Receiver<Option<Duration>>,
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
            }
        }
    })
}

/// Live source backed by one persistent sysinfo handle.
pub struct SystemResourceSource {
    system: System,
    disks: Disks,
    last_sample: Option<Instant>,
    disk_peak_bytes_per_second: f64,
    cpu_monitor: medulla_monitor::CpuMonitor,
}

impl Default for SystemResourceSource {
    fn default() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new(),
            last_sample: None,
            disk_peak_bytes_per_second: 1.0,
            cpu_monitor: medulla_monitor::CpuMonitor::default(),
        }
    }
}

impl SystemResourceSource {
    /// Keep the IO bar scaled to recent activity instead of only this sample.
    fn update_disk_peak(&mut self, read_rate: f64, write_rate: f64) -> f64 {
        self.disk_peak_bytes_per_second = (self.disk_peak_bytes_per_second * 0.9)
            .max(read_rate)
            .max(write_rate)
            .max(1.0);
        self.disk_peak_bytes_per_second
    }
}

impl ResourceSource for SystemResourceSource {
    fn sample(&mut self) -> ResourceSnapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sample
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(0.0);
        let baseline_ready = self.last_sample.replace(now).is_some();
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_tasks(),
        );
        self.disks = Disks::new_with_refreshed_list();
        let task_pids: std::collections::HashSet<u32> = self
            .system
            .processes()
            .values()
            .filter_map(|process| process.tasks())
            .flatten()
            .map(|pid| pid.as_u32())
            .collect();
        let processes: Vec<ProcessSnapshot> = self
            .system
            .processes()
            .iter()
            .filter(|(pid, _)| !task_pids.contains(&pid.as_u32()))
            .map(|(pid, process)| ProcessSnapshot {
                pid: pid.as_u32(),
                ppid: process.parent().map(|parent| parent.as_u32()),
                exe: process
                    .exe()
                    .map(|path| path.to_string_lossy().into_owned()),
                cmd: process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect(),
                cwd: process
                    .cwd()
                    .map(|path| path.to_string_lossy().into_owned()),
                start_time: process.start_time(),
                cpu_percent: f64::from(process.cpu_usage()),
                memory_bytes: process.memory(),
                disk_read_bytes: if baseline_ready {
                    process.disk_usage().read_bytes
                } else {
                    0
                },
                disk_written_bytes: if baseline_ready {
                    process.disk_usage().written_bytes
                } else {
                    0
                },
                threads: process.tasks().map_or(0, |tasks| tasks.len()),
            })
            .collect();
        let process_index = std::sync::Arc::new(
            processes
                .iter()
                .enumerate()
                .map(|(index, process)| (process.pid, index))
                .collect(),
        );
        let mut children = std::collections::HashMap::<u32, Vec<u32>>::new();
        for process in &processes {
            if let Some(parent) = process.ppid {
                children.entry(parent).or_default().push(process.pid);
            }
        }
        let current_pid = sysinfo::get_current_pid().ok().map(|pid| pid.as_u32());
        let current_process = current_pid.and_then(|pid| {
            let process = self
                .system
                .processes()
                .iter()
                .find(|(candidate, _)| candidate.as_u32() == pid)
                .map(|(_, process)| process)?;
            let cores = self.system.cpus().len().max(1) as f64;
            let cpu_percent = f64::from(process.cpu_usage());
            let memory_bytes = process.memory();
            let total_memory_bytes = self.system.total_memory();
            let disk = process.disk_usage();
            let seconds = elapsed.max(0.001);
            let read_rate = if baseline_ready {
                disk.read_bytes as f64 / seconds
            } else {
                0.0
            };
            let write_rate = if baseline_ready {
                disk.written_bytes as f64 / seconds
            } else {
                0.0
            };
            let disk_peak = self.update_disk_peak(read_rate, write_rate);
            Some(crate::ui::resources::types::ResourceSnapshot {
                cpu_fraction: (cpu_percent / (cores * 100.0)).clamp(0.0, 1.0),
                memory_bytes,
                total_memory_bytes,
                disk_read_bytes_per_second: read_rate,
                disk_write_bytes_per_second: write_rate,
                disk_peak_bytes_per_second: disk_peak,
            })
        });
        let mut cpu = self.cpu_monitor.sample();
        if cpu.cores.is_empty() {
            cpu.total.total = f32::from(self.system.global_cpu_usage());
            cpu.cores = self.system.cpus().iter().map(|cpu| medulla_monitor::CpuReading {
                total: f32::from(cpu.cpu_usage()), ..Default::default()
            }).collect();
        }
        ResourceSnapshot {
            cpu,
            current_process,
            history: Default::default(),
            process_index,
            children_index: std::sync::Arc::new(children),
            sample_interval_seconds: elapsed,
            cpu_percent: f64::from(self.system.global_cpu_usage()),
            cpu_cores_percent: self
                .system
                .cpus()
                .iter()
                .map(|cpu| f64::from(cpu.cpu_usage()))
                .collect(),
            memory_total_bytes: self.system.total_memory(),
            memory_used_bytes: self.system.used_memory(),
            memory_available_bytes: self.system.available_memory(),
            memory_free_bytes: self.system.free_memory(),
            swap_total_bytes: self.system.total_swap(),
            swap_used_bytes: self.system.used_swap(),
            disks: self
                .disks
                .list()
                .iter()
                .map(|disk| DiskSnapshot {
                    mount: disk.mount_point().to_string_lossy().into_owned(),
                    total_bytes: disk.total_space(),
                    used_bytes: disk.total_space().saturating_sub(disk.available_space()),
                })
                .collect(),
            processes,
        }
    }
}
