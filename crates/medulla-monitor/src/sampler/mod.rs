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
    cpu_monitor: crate::CpuMonitor,
    disk_options: DiskOptions,
    #[cfg(target_os = "linux")]
    linux: LinuxResourceMonitor,
}

impl Default for SystemResourceSource {
    fn default() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new(),
            last_sample: None,
            disk_peak_bytes_per_second: 1.0,
            cpu_monitor: crate::CpuMonitor::default(),
            disk_options: DiskOptions::default(),
            #[cfg(target_os = "linux")]
            linux: LinuxResourceMonitor::default(),
        }
    }
}

/// Optional disk list filtering, configured by the TUI without coupling this crate to its config type.
#[derive(Debug, Clone, Default)]
pub struct DiskOptions {
    pub only_physical: bool,
    pub use_fstab: bool,
    pub filter: String,
}

impl SystemResourceSource {
    /// Set mount filtering used when building each resource snapshot.
    pub fn with_disk_options(mut self, options: DiskOptions) -> Self {
        self.disk_options = options;
        self
    }
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
        self.system.refresh_cpu_all();
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
        self.disks.refresh(true);
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
            Some(crate::ResourceSnapshot {
                cpu_fraction: (cpu_percent / (cores * 100.0)).clamp(0.0, 1.0),
                memory_bytes,
                total_memory_bytes,
                disk_read_bytes_per_second: read_rate,
                disk_write_bytes_per_second: write_rate,
                disk_peak_bytes_per_second: disk_peak,
            })
        });
        let mut cpu = self.cpu_monitor.sample();
        cpu.model = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|brand| !brand.is_empty());
        if cpu.cores.is_empty() {
            cpu.total.total = self.system.global_cpu_usage();
            cpu.cores = self
                .system
                .cpus()
                .iter()
                .map(|cpu| crate::CpuReading {
                    total: cpu.cpu_usage(),
                    ..Default::default()
                })
                .collect();
        }
        if cpu.frequencies_mhz.iter().all(Option::is_none) {
            cpu.frequencies_mhz = self
                .system
                .cpus()
                .iter()
                .map(|cpu| Some(cpu.frequency()))
                .collect();
        }
        if cpu.load_average == [0.0; 3] {
            let load = System::load_average();
            cpu.load_average = [load.one, load.five, load.fifteen];
        }
        if cpu.uptime_seconds.is_none() {
            cpu.uptime_seconds = Some(System::uptime());
        }
        #[cfg(target_os = "linux")]
        let linux = self.linux.sample(&self.disks, elapsed);
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
            memory_total_bytes: {
                #[cfg(target_os = "linux")]
                {
                    linux.memory.total
                }
                #[cfg(not(target_os = "linux"))]
                {
                    self.system.total_memory()
                }
            },
            memory_used_bytes: {
                #[cfg(target_os = "linux")]
                {
                    linux.memory.total.saturating_sub(linux.memory.available)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    self.system.used_memory()
                }
            },
            memory_available_bytes: {
                #[cfg(target_os = "linux")]
                {
                    linux.memory.available
                }
                #[cfg(not(target_os = "linux"))]
                {
                    self.system.available_memory()
                }
            },
            memory_cached_bytes: {
                #[cfg(target_os = "linux")]
                {
                    linux.memory.cached
                }
                #[cfg(not(target_os = "linux"))]
                {
                    0
                }
            },
            memory_free_bytes: {
                #[cfg(target_os = "linux")]
                {
                    linux.memory.free
                }
                #[cfg(not(target_os = "linux"))]
                {
                    self.system.free_memory()
                }
            },
            swap_total_bytes: self.system.total_swap(),
            swap_used_bytes: self.system.used_swap(),
            disks: self
                .disks
                .list()
                .iter()
                .filter(|disk| disk_is_visible(disk, &self.disk_options))
                .map(|disk| {
                    #[cfg(target_os = "linux")]
                    let io = linux
                        .io
                        .get(disk.name().to_string_lossy().trim_start_matches("/dev/"))
                        .copied();
                    DiskSnapshot {
                        device: disk.name().to_string_lossy().into_owned(),
                        mount: disk.mount_point().to_string_lossy().into_owned(),
                        total_bytes: disk.total_space(),
                        used_bytes: disk.total_space().saturating_sub(disk.available_space()),
                        #[cfg(target_os = "linux")]
                        read_bytes_per_second: io.map_or(0.0, |io| io.read_rate),
                        #[cfg(not(target_os = "linux"))]
                        read_bytes_per_second: disk.usage().read_bytes as f64 / elapsed.max(0.001),
                        #[cfg(target_os = "linux")]
                        write_bytes_per_second: io.map_or(0.0, |io| io.write_rate),
                        #[cfg(not(target_os = "linux"))]
                        write_bytes_per_second: disk.usage().written_bytes as f64
                            / elapsed.max(0.001),
                        #[cfg(target_os = "linux")]
                        busy_percent: io.map(|io| io.busy),
                        #[cfg(not(target_os = "linux"))]
                        busy_percent: None,
                    }
                })
                .collect(),
            processes,
        }
    }
}

fn disk_is_visible(disk: &sysinfo::Disk, options: &DiskOptions) -> bool {
    let mount = disk.mount_point().to_string_lossy();
    let device = disk.name().to_string_lossy();
    let fs = disk.file_system().to_string_lossy();
    #[cfg(target_os = "linux")]
    let fstab = if options.use_fstab {
        std::fs::read_to_string("/etc/fstab").unwrap_or_default()
    } else {
        String::new()
    };
    #[cfg(not(target_os = "linux"))]
    let fstab = String::new();
    #[cfg(not(target_os = "linux"))]
    let options = &DiskOptions {
        use_fstab: false,
        ..options.clone()
    };
    mount_is_visible(&mount, &device, &fs, options, &fstab)
}

fn mount_is_visible(
    mount: &str,
    device: &str,
    filesystem: &str,
    options: &DiskOptions,
    fstab: &str,
) -> bool {
    if options.only_physical {
        let fs = filesystem.to_ascii_lowercase();
        if [
            "tmpfs", "devtmpfs", "overlay", "squashfs", "proc", "sysfs", "cgroup",
        ]
        .iter()
        .any(|virtual_fs| fs == *virtual_fs)
            || device.starts_with("/dev/loop")
            || device.starts_with("/dev/ram")
        {
            return false;
        }
    }
    if options.use_fstab {
        let mounts: std::collections::HashSet<String> = fstab
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next()?.trim();
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() >= 2).then(|| fields[1].replace("\\040", " "))
            })
            .collect();
        if !mounts.contains(mount) {
            return false;
        }
    }
    let filters: Vec<_> = options
        .filter
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    for filter in filters.iter().filter_map(|s| s.strip_prefix("exclude=")) {
        if mount == filter || mount.starts_with(&format!("{}/", filter.trim_end_matches('/'))) {
            return false;
        }
    }
    let includes: Vec<_> = filters
        .iter()
        .filter(|s| !s.starts_with("exclude="))
        .copied()
        .collect();
    includes.is_empty()
        || includes.iter().any(|prefix| {
            mount == *prefix || mount.starts_with(&format!("{}/", prefix.trim_end_matches('/')))
        })
}

#[cfg(target_os = "linux")]
struct LinuxResourceMonitor {
    root: std::path::PathBuf,
    previous: std::collections::HashMap<String, (u64, u64, u64)>,
    previous_uptime: Option<f64>,
}

#[cfg(target_os = "linux")]
struct LinuxSample {
    memory: MemoryReading,
    io: std::collections::HashMap<String, IoReading>,
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct MemoryReading {
    total: u64,
    available: u64,
    free: u64,
    cached: u64,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct IoReading {
    read_rate: f64,
    write_rate: f64,
    busy: f64,
}

#[cfg(target_os = "linux")]
impl Default for LinuxResourceMonitor {
    fn default() -> Self {
        Self {
            root: "/".into(),
            previous: Default::default(),
            previous_uptime: None,
        }
    }
}

#[cfg(target_os = "linux")]
impl LinuxResourceMonitor {
    fn sample(&mut self, disks: &Disks, elapsed: f64) -> LinuxSample {
        use std::fs;
        let mem = fs::read_to_string(self.root.join("proc/meminfo")).unwrap_or_default();
        let memory = parse_memory(&mem);
        let uptime = fs::read_to_string(self.root.join("proc/uptime"))
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let seconds = self
            .previous_uptime
            .replace(uptime)
            .map(|old| (uptime - old).max(0.001))
            .unwrap_or(elapsed.max(0.001));
        let mut io = std::collections::HashMap::new();
        for disk in disks.list() {
            let name = disk
                .name()
                .to_string_lossy()
                .trim_start_matches("/dev/")
                .to_owned();
            let stat_path = self.root.join("sys/block").join(&name).join("stat");
            let Ok(stat) = fs::read_to_string(stat_path) else {
                continue;
            };
            let Some(now) = parse_block_stat(&stat) else { continue };
            if let Some(old) = self.previous.insert(name.clone(), now) {
                io.insert(name, io_delta(old, now, seconds));
            }
        }
        LinuxSample { memory, io }
    }
}

#[cfg(target_os = "linux")]
fn parse_memory(contents: &str) -> MemoryReading {
    let fields: std::collections::HashMap<&str, u64> = contents
        .lines()
        .filter_map(|line| {
            let mut p = line.split_whitespace();
            let (key, value) = (p.next()?, p.next()?.parse::<u64>().ok()?);
            Some((key.trim_end_matches(':'), value.saturating_mul(1024)))
        })
        .collect();
    let total = fields.get("MemTotal").copied().unwrap_or(0);
    let free = fields.get("MemFree").copied().unwrap_or(0);
    let cached = fields.get("Cached").copied().unwrap_or(0);
    let available = fields
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| free.saturating_add(cached))
        .min(total);
    MemoryReading {
        total,
        available,
        free,
        cached,
    }
}

#[cfg(target_os = "linux")]
fn io_delta(old: (u64, u64, u64), new: (u64, u64, u64), seconds: f64) -> IoReading {
    let seconds = seconds.max(0.001);
    IoReading {
        read_rate: new.0.saturating_sub(old.0).saturating_mul(512) as f64 / seconds,
        write_rate: new.1.saturating_sub(old.1).saturating_mul(512) as f64 / seconds,
        busy: (new.2.saturating_sub(old.2) as f64 / seconds / 10.0).clamp(0.0, 100.0),
    }
}

#[cfg(target_os = "linux")]
fn parse_block_stat(contents: &str) -> Option<(u64, u64, u64)> {
    let values: Vec<u64> = contents.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
    (values.len() >= 10).then(|| (values[2], values[6], values[9]))
}
