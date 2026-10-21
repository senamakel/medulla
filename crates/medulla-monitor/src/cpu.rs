//! Whole-machine CPU sampling, with Linux's `/proc/stat` as the precise source.

#[cfg(target_os = "linux")]
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// CPU time fields exposed by Linux `/proc/stat` and selectable in graphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuField {
    Total,
    User,
    Nice,
    System,
    Idle,
    Iowait,
    Irq,
    Softirq,
    Steal,
    Guest,
    GuestNice,
}

impl CpuField {
    /// Parse a graph field name.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "total" => Self::Total,
            "user" => Self::User,
            "nice" => Self::Nice,
            "system" => Self::System,
            "idle" => Self::Idle,
            "iowait" => Self::Iowait,
            "irq" => Self::Irq,
            "softirq" => Self::Softirq,
            "steal" => Self::Steal,
            "guest" => Self::Guest,
            "guest_nice" => Self::GuestNice,
            _ => return None,
        })
    }
}

/// CPU readings for one sample, represented as percentages from 0 to 100.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuReading {
    pub total: f32,
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub idle: f32,
    pub iowait: f32,
    pub irq: f32,
    pub softirq: f32,
    pub steal: f32,
    pub guest: f32,
    pub guest_nice: f32,
}

impl CpuReading {
    /// Return the value for a selectable graph field.
    pub fn field(&self, field: CpuField) -> f32 {
        match field {
            CpuField::Total => self.total,
            CpuField::User => self.user,
            CpuField::Nice => self.nice,
            CpuField::System => self.system,
            CpuField::Idle => self.idle,
            CpuField::Iowait => self.iowait,
            CpuField::Irq => self.irq,
            CpuField::Softirq => self.softirq,
            CpuField::Steal => self.steal,
            CpuField::Guest => self.guest,
            CpuField::GuestNice => self.guest_nice,
        }
    }
}

/// Immutable whole-machine CPU data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuSnapshot {
    pub model: Option<String>,
    pub total: CpuReading,
    pub cores: Vec<CpuReading>,
    pub frequencies_mhz: Vec<u64>,
    pub package_temperature_c: Option<f32>,
    pub package_temperature_crit_c: Option<f32>,
    pub core_temperatures_c: Vec<(usize, f32, f32)>,
    pub load_average: [f64; 3],
    pub uptime_seconds: Option<u64>,
    pub battery_percent: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg(target_os = "linux")]
struct Times([u64; 10]);

#[cfg(target_os = "linux")]
type CoreTemperatureReading = (usize, f32, f32);
#[cfg(target_os = "linux")]
type TemperatureReadings = (Option<f32>, Option<f32>, Vec<CoreTemperatureReading>);

/// Stateful CPU sampler. Linux files can be overridden to make it deterministic in tests.
pub struct CpuMonitor {
    #[cfg(target_os = "linux")]
    root: PathBuf,
    #[cfg(target_os = "linux")]
    previous: BTreeMap<String, Times>,
}

impl Default for CpuMonitor {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            root: PathBuf::from("/"),
            #[cfg(target_os = "linux")]
            previous: BTreeMap::new(),
        }
    }
}

impl CpuMonitor {
    /// Use a filesystem root containing `proc` and `sys`; primarily useful for fixtures.
    #[cfg(target_os = "linux")]
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            previous: BTreeMap::new(),
        }
    }

    /// Read CPU readings and the Linux-only supplementary data when available.
    pub fn sample(&mut self) -> CpuSnapshot {
        #[cfg(target_os = "linux")]
        {
            self.sample_linux()
        }
        #[cfg(not(target_os = "linux"))]
        {
            CpuSnapshot::default()
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_linux(&mut self) -> CpuSnapshot {
        let root = self.root.clone();
        let stat = fs::read_to_string(root.join("proc/stat")).unwrap_or_default();
        let mut readings = Vec::new();
        for line in stat.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next().filter(|n| {
                *n == "cpu"
                    || n.strip_prefix("cpu")
                        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            }) else {
                continue;
            };
            let values: Vec<u64> = parts
                .filter_map(|part| part.parse().ok())
                .take(10)
                .collect();
            if values.len() < 4 {
                continue;
            }
            let mut raw = [0; 10];
            raw[..values.len()].copy_from_slice(&values);
            let current = Times(raw);
            let previous = self.previous.insert(name.to_owned(), current);
            readings.push((
                name.to_owned(),
                previous
                    .map(|old| cpu_delta(old, current))
                    .unwrap_or_default(),
            ));
        }
        let total = readings
            .iter()
            .find(|(name, _)| name == "cpu")
            .map(|(_, reading)| reading.clone())
            .unwrap_or_default();
        let cores = readings
            .into_iter()
            .filter(|(name, _)| name != "cpu")
            .map(|(_, reading)| reading)
            .collect();
        let (package_temperature_c, package_temperature_crit_c, core_temperatures_c) =
            read_temperatures(&root);
        CpuSnapshot {
            total,
            cores,
            frequencies_mhz: read_frequencies(&root),
            package_temperature_c,
            package_temperature_crit_c,
            core_temperatures_c,
            load_average: read_load(&root),
            uptime_seconds: read_uptime(&root),
            battery_percent: read_battery(&root),
        }
    }
}

#[cfg(target_os = "linux")]
fn cpu_delta(old: Times, new: Times) -> CpuReading {
    let mut delta = [0f64; 10];
    for (index, value) in delta.iter_mut().enumerate() {
        *value = new.0[index].saturating_sub(old.0[index]) as f64;
    }
    // Linux reports guest time inside user/nice. Remove it from total and user/nice fields.
    let total = delta.iter().sum::<f64>() - delta[8] - delta[9];
    let idle = delta[3] + delta[4];
    let pct = |v: f64| {
        if total > 0.0 {
            (v * 100.0 / total) as f32
        } else {
            0.0
        }
    };
    CpuReading {
        total: pct(total - idle),
        user: pct(delta[0] - delta[8]),
        nice: pct(delta[1] - delta[9]),
        system: pct(delta[2]),
        idle: pct(delta[3]),
        iowait: pct(delta[4]),
        irq: pct(delta[5]),
        softirq: pct(delta[6]),
        steal: pct(delta[7]),
        guest: pct(delta[8]),
        guest_nice: pct(delta[9]),
    }
}

#[cfg(target_os = "linux")]
fn read_frequencies(root: &Path) -> Vec<u64> {
    let base = root.join("sys/devices/system/cpu");
    let Ok(entries) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let index = name.strip_prefix("cpu")?.parse::<usize>().ok()?;
            let value = fs::read_to_string(entry.path().join("cpufreq/scaling_cur_freq"))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()?;
            Some((index, value / 1000))
        })
        .collect();
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, hz)| hz).collect()
}

#[cfg(target_os = "linux")]
fn read_temperatures(root: &Path) -> TemperatureReadings {
    let mut package = None;
    let mut package_crit = None;
    let mut cores = Vec::new();
    let Ok(devices) = fs::read_dir(root.join("sys/class/hwmon")) else {
        return (package, package_crit, cores);
    };
    for device in devices.flatten() {
        let path = fs::canonicalize(device.path()).unwrap_or_else(|_| device.path());
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(index) = name
                .strip_prefix("temp")
                .and_then(|s| s.strip_suffix("_input"))
                .and_then(|s| s.parse::<usize>().ok())
            else {
                continue;
            };
            let prefix = format!("temp{index}");
            let label = fs::read_to_string(path.join(format!("{prefix}_label")))
                .unwrap_or_else(|_| prefix.clone())
                .trim()
                .to_owned();
            let Ok(milli) = fs::read_to_string(entry.path())
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .ok_or(())
            else {
                continue;
            };
            let temp = milli as f32 / 1000.0;
            let crit = fs::read_to_string(path.join(format!("{prefix}_crit")))
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .map(|v| v / 1000.0)
                .unwrap_or(90.0);
            let lower = label.to_ascii_lowercase();
            if lower.starts_with("package id")
                || lower.starts_with("tdie")
                || lower.starts_with("soc temperature")
            {
                if package.is_none() {
                    package = Some(temp);
                    package_crit = Some(crit);
                }
            } else if let Some(core) = lower
                .strip_prefix("core")
                .and_then(|s| s.trim().parse::<usize>().ok())
            {
                cores.push((core, temp, crit));
            }
        }
    }
    cores.sort_by_key(|(core, _, _)| *core);
    (package, package_crit, cores)
}

#[cfg(target_os = "linux")]
fn read_load(root: &Path) -> [f64; 3] {
    let s = fs::read_to_string(root.join("proc/loadavg")).unwrap_or_default();
    let mut p = s.split_whitespace().filter_map(|s| s.parse().ok());
    [
        p.next().unwrap_or(0.0),
        p.next().unwrap_or(0.0),
        p.next().unwrap_or(0.0),
    ]
}
#[cfg(target_os = "linux")]
fn read_uptime(root: &Path) -> Option<u64> {
    fs::read_to_string(root.join("proc/uptime"))
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(|v| v as u64)
}
#[cfg(target_os = "linux")]
fn read_battery(root: &Path) -> Option<f32> {
    for entry in fs::read_dir(root.join("sys/class/power_supply"))
        .ok()?
        .flatten()
    {
        let path = entry.path();
        if fs::read_to_string(path.join("type"))
            .ok()
            .as_deref()
            .map(str::trim)
            != Some("Battery")
        {
            continue;
        }
        if let Ok(capacity) = fs::read_to_string(path.join("capacity")) {
            return capacity.trim().parse().ok();
        }
        let now = fs::read_to_string(path.join("energy_now"))
            .or_else(|_| fs::read_to_string(path.join("charge_now")))
            .ok()?
            .trim()
            .parse::<f32>()
            .ok()?;
        let full = fs::read_to_string(path.join("energy_full"))
            .or_else(|_| fs::read_to_string(path.join("charge_full")))
            .ok()?
            .trim()
            .parse::<f32>()
            .ok()?;
        if full > 0.0 {
            return Some((now * 100.0 / full).clamp(0.0, 100.0));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_cpu_percent_uses_guest_adjusted_totals_and_field_deltas() {
        let reading = cpu_delta(
            Times([10, 0, 10, 60, 0, 0, 0, 0, 0, 0]),
            Times([30, 0, 20, 110, 10, 0, 0, 0, 5, 0]),
        );
        assert!((reading.total - 33.333_332).abs() < 0.001);
        assert!((reading.user - 16.666_666).abs() < 0.001);
        assert!((reading.system - 11.111_111).abs() < 0.001);
        assert!((reading.idle - 55.555_557).abs() < 0.001);
        assert!((reading.iowait - 11.111_111).abs() < 0.001);
        assert!((reading.guest - 5.555_556).abs() < 0.001);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_collector_reads_pair_deltas_and_hwmon_labels() {
        let temp = tempfile::tempdir().unwrap();
        let write = |path: &str, value: &str| {
            let path = temp.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        };
        write(
            "proc/stat",
            "cpu 30 0 20 110 10 0 0 0 5 0\ncpu0 20 0 10 50 5 0 0 0 5 0\n",
        );
        write("proc/loadavg", "0.50 0.25 0.10 1/20 100\n");
        write("proc/uptime", "1234.5 0\n");
        let hwmon = temp.path().join("sys/class/hwmon/hwmon0");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(hwmon.join("temp1_input"), "65000\n").unwrap();
        fs::write(hwmon.join("temp1_label"), "Package id 0\n").unwrap();
        fs::write(hwmon.join("temp1_crit"), "100000\n").unwrap();
        fs::write(hwmon.join("temp2_input"), "60000\n").unwrap();
        fs::write(hwmon.join("temp2_label"), "Core 0\n").unwrap();
        let mut monitor = CpuMonitor::with_root(temp.path());
        let first = monitor.sample();
        assert_eq!(first.total, CpuReading::default());
        write(
            "proc/stat",
            "cpu 50 0 30 160 20 0 0 0 10 0\ncpu0 30 0 20 70 10 0 0 0 10 0\n",
        );
        let second = monitor.sample();
        assert!((second.total.total - 33.333_332).abs() < 0.001);
        assert_eq!(second.cores.len(), 1);
        assert_eq!(second.package_temperature_c, Some(65.0));
        assert_eq!(second.package_temperature_crit_c, Some(100.0));
        assert_eq!(second.core_temperatures_c, [(0, 60.0, 90.0)]);
        assert_eq!(second.load_average, [0.5, 0.25, 0.1]);
        assert_eq!(second.uptime_seconds, Some(1234));
    }
}
