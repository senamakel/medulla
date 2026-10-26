//! Linux readings sysinfo does not provide: `/proc` memory and `/sys` block
//! and interface state.

use super::*;

#[cfg(target_os = "linux")]
pub(super) fn linux_interface_flags(name: &str) -> Option<u32> {
    std::fs::read_to_string(
        std::path::Path::new("/sys/class/net")
            .join(name)
            .join("flags"),
    )
    .ok()
    .and_then(|flags| u32::from_str_radix(flags.trim().trim_start_matches("0x"), 16).ok())
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn linux_interface_state(flags: u32) -> (bool, bool) {
    (flags & 0x1 != 0, flags & 0x40 != 0)
}

#[cfg(target_os = "linux")]
pub(super) struct LinuxResourceMonitor {
    pub(super) root: std::path::PathBuf,
    pub(super) previous: std::collections::HashMap<String, (u64, u64, u64)>,
    pub(super) previous_uptime: Option<f64>,
}

#[cfg(target_os = "linux")]
pub(super) struct LinuxSample {
    pub(super) memory: MemoryReading,
    pub(super) io: std::collections::HashMap<String, IoReading>,
}

#[cfg(target_os = "linux")]
#[derive(Default)]
pub(super) struct MemoryReading {
    pub(super) total: u64,
    pub(super) available: u64,
    pub(super) free: u64,
    pub(super) cached: u64,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
pub(super) struct IoReading {
    pub(super) read_rate: f64,
    pub(super) write_rate: f64,
    pub(super) busy: f64,
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
    pub(super) fn sample(&mut self, disks: &Disks, elapsed: f64) -> LinuxSample {
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
            let Some(now) = read_block_stat(&self.root, &name) else {
                continue;
            };
            if let Some(old) = self.previous.insert(name.clone(), now) {
                io.insert(name, io_delta(old, now, seconds));
            }
        }
        LinuxSample { memory, io }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn read_block_stat(root: &std::path::Path, name: &str) -> Option<(u64, u64, u64)> {
    use std::fs;
    let stat = fs::read_to_string(root.join("sys/class/block").join(name).join("stat"))
        .or_else(|_| fs::read_to_string(root.join("sys/block").join(name).join("stat")))
        .ok()?;
    parse_block_stat(&stat)
}

#[cfg(target_os = "linux")]
pub(super) fn parse_memory(contents: &str) -> MemoryReading {
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
pub(super) fn io_delta(old: (u64, u64, u64), new: (u64, u64, u64), seconds: f64) -> IoReading {
    let seconds = seconds.max(0.001);
    IoReading {
        read_rate: new.0.saturating_sub(old.0).saturating_mul(512) as f64 / seconds,
        write_rate: new.1.saturating_sub(old.1).saturating_mul(512) as f64 / seconds,
        busy: (new.2.saturating_sub(old.2) as f64 / seconds / 10.0).clamp(0.0, 100.0),
    }
}

#[cfg(target_os = "linux")]
pub(super) fn parse_block_stat(contents: &str) -> Option<(u64, u64, u64)> {
    let values: Vec<u64> = contents
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (values.len() >= 10).then(|| (values[2], values[6], values[9]))
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
