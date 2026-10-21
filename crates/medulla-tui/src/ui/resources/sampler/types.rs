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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceSnapshot {
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
