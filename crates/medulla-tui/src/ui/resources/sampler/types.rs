//! Snapshot data returned by the background resource sampler.

/// Point-in-time disk capacity reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSnapshot {
    /// Mount point or volume name.
    pub mount: String,
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Capacity in use in bytes.
    pub used_bytes: u64,
}

/// Point-in-time process reading.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSnapshot {
    /// Process identifier.
    pub pid: u32,
    /// Parent process identifier when available.
    pub ppid: Option<u32>,
    /// Executable path when available.
    pub exe: Option<String>,
    /// Command line arguments.
    pub cmd: Vec<String>,
    /// Current working directory when available.
    pub cwd: Option<String>,
    /// Start time as seconds since Unix epoch.
    pub start_time: u64,
    /// CPU utilization as a percentage.
    pub cpu_percent: f64,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// Bytes read since the previous system refresh.
    pub disk_read_bytes: u64,
    /// Bytes written since the previous system refresh.
    pub disk_written_bytes: u64,
    /// Number of threads/tasks reported for this process.
    pub threads: usize,
}

/// Immutable whole-device readings suitable for sharing with renderers.
#[derive(Debug, Clone, Default)]
pub struct ResourceSnapshot {
    /// Current process metrics used by the status line.
    pub current_process: Option<super::super::ResourceSnapshot>,
    /// Recent readings accumulated by the sampler for graph rendering.
    pub history: std::sync::Arc<ResourceHistory>,
    /// Seconds since the previous sample, used to scale process I/O deltas.
    pub sample_interval_seconds: f64,
    /// Total CPU utilization as a percentage.
    pub cpu_percent: f64,
    /// Utilization percentages for each logical CPU.
    pub cpu_cores_percent: Vec<f64>,
    /// Physical memory totals and readings, in bytes.
    pub memory_total_bytes: u64,
    /// Physical memory currently in use, in bytes.
    pub memory_used_bytes: u64,
    /// Physical memory available to applications, in bytes.
    pub memory_available_bytes: u64,
    /// Physical memory currently unused, in bytes.
    pub memory_free_bytes: u64,
    /// Total swap capacity, in bytes.
    pub swap_total_bytes: u64,
    /// Swap in use, in bytes.
    pub swap_used_bytes: u64,
    /// Mounted disk readings.
    pub disks: Vec<DiskSnapshot>,
    /// Process table in the order sysinfo supplied it.
    pub processes: Vec<ProcessSnapshot>,
}

impl ResourceSnapshot {
    /// Adapt whole-device readings for the existing compact footer.
    pub fn device_snapshot(&self) -> super::super::DeviceSnapshot {
        let cwd = std::env::current_dir().ok();
        let disk = self
            .disks
            .iter()
            .filter_map(|disk| {
                let mount = std::path::Path::new(&disk.mount);
                let depth = cwd.as_ref()?.strip_prefix(mount).ok()?.components().count();
                Some((depth, disk))
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, disk)| disk);
        super::super::DeviceSnapshot {
            cpu_fraction: Some((self.cpu_percent / 100.0).clamp(0.0, 1.0)),
            memory_used_bytes: Some(self.memory_used_bytes),
            memory_total_bytes: Some(self.memory_total_bytes),
            disk_used_bytes: disk.map(|disk| disk.used_bytes),
            disk_total_bytes: disk.map(|disk| disk.total_bytes),
        }
    }

    /// Aggregate the cached process table for a session, without touching the
    /// host from the render path.
    pub fn session_snapshot(&self, root_pid: u32) -> Option<super::super::SessionSnapshot> {
        let process_by_pid: std::collections::HashMap<_, _> = self
            .processes
            .iter()
            .map(|process| (process.pid, process))
            .collect();
        let root = *process_by_pid.get(&root_pid)?;
        let mut children = std::collections::HashMap::<u32, Vec<u32>>::new();
        for process in &self.processes {
            if let Some(parent) = process.ppid {
                children.entry(parent).or_default().push(process.pid);
            }
        }
        let core_count = self.cpu_cores_percent.len().max(1) as f64;
        let mut result = super::super::SessionSnapshot {
            total_memory_bytes: self.memory_total_bytes,
            ..Default::default()
        };
        let mut pending = vec![root.pid];
        let mut visited = std::collections::HashSet::new();
        while let Some(pid) = pending.pop() {
            if !visited.insert(pid) {
                continue;
            }
            let Some(process) = process_by_pid.get(&pid).copied() else {
                continue;
            };
            result.processes += 1;
            result.cpu_fraction += process.cpu_percent / (core_count * 100.0);
            result.memory_bytes = result.memory_bytes.saturating_add(process.memory_bytes);
            result.disk_bytes_per_second += (process.disk_read_bytes + process.disk_written_bytes)
                as f64
                / self.sample_interval_seconds.max(0.001);
            if let Some(descendants) = children.get(&pid) {
                pending.extend(descendants);
            }
        }
        result.cpu_fraction = result.cpu_fraction.clamp(0.0, 1.0);
        Some(result)
    }
}

/// Recent scalar series shared between the sampler and rendering views.
#[derive(Debug, Clone, Default)]
pub struct ResourceHistory {
    /// Whole-device CPU utilization percentages.
    pub cpu_percent: super::History<f64>,
    /// Physical memory currently used, in bytes.
    pub memory_used_bytes: super::History<u64>,
    /// Swap currently used, in bytes.
    pub swap_used_bytes: super::History<u64>,
    /// Number of visible processes.
    pub process_count: super::History<usize>,
}

impl ResourceHistory {
    /// Append one snapshot and trim every series to the requested viewport size.
    pub fn push(&mut self, snapshot: &ResourceSnapshot, capacity: usize) {
        self.cpu_percent.push(snapshot.cpu_percent, capacity);
        self.memory_used_bytes
            .push(snapshot.memory_used_bytes, capacity);
        self.swap_used_bytes
            .push(snapshot.swap_used_bytes, capacity);
        self.process_count.push(snapshot.processes.len(), capacity);
    }
}
