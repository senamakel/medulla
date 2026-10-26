//! The disks a sample lists, with each one's IO where the platform has it.

#[cfg(not(target_os = "linux"))]
use super::source::per_second;
use super::*;

/// What one sample needs to turn sysinfo's disk counters into rates. Linux
/// reads its rates from `/proc` instead.
#[cfg(not(target_os = "linux"))]
pub(super) struct DiskRates {
    pub elapsed: f64,
    pub baseline_ready: bool,
}

/// Build the visible disks, in the order btop lists them.
pub(super) fn disk_snapshots(
    disks: &Disks,
    options: &DiskOptions,
    #[cfg(not(target_os = "linux"))] rates: DiskRates,
    #[cfg(target_os = "linux")] io: &std::collections::HashMap<String, IoReading>,
) -> Vec<DiskSnapshot> {
    #[cfg(target_os = "macos")]
    {
        let mounted = macos_disks(disks, options, &rates);
        if !mounted.is_empty() {
            return mounted;
        }
    }
    disks
        .list()
        .iter()
        .filter(|disk| disk_is_visible(disk, options))
        .map(|disk| {
            #[cfg(target_os = "linux")]
            let io = io
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
                read_bytes_per_second: per_second(
                    disk.usage().read_bytes,
                    rates.elapsed,
                    rates.baseline_ready,
                ),
                #[cfg(target_os = "linux")]
                write_bytes_per_second: io.map_or(0.0, |io| io.write_rate),
                #[cfg(not(target_os = "linux"))]
                write_bytes_per_second: per_second(
                    disk.usage().written_bytes,
                    rates.elapsed,
                    rates.baseline_ready,
                ),
                #[cfg(target_os = "linux")]
                busy_percent: io.map(|io| io.busy),
                #[cfg(not(target_os = "linux"))]
                busy_percent: None,
                #[cfg(target_os = "linux")]
                has_io: io.is_some(),
                #[cfg(not(target_os = "linux"))]
                has_io: true,
            }
        })
        .collect()
}

/// Every local volume from the mount table, each with the IO of the physical
/// disk it lives on. sysinfo measures IO per whole disk, so one reading per
/// disk serves every volume on it; a disk sysinfo does not list has none.
#[cfg(target_os = "macos")]
fn macos_disks(disks: &Disks, options: &DiskOptions, rates: &DiskRates) -> Vec<DiskSnapshot> {
    let mut io = std::collections::HashMap::<String, (f64, f64)>::new();
    for disk in disks.list() {
        let Some(device) = super::macos::device_of(disk.mount_point()) else {
            continue;
        };
        let Some(whole) = super::macos::whole_disk(&device) else {
            continue;
        };
        let usage = disk.usage();
        io.entry(whole.to_owned()).or_insert((
            per_second(usage.read_bytes, rates.elapsed, rates.baseline_ready),
            per_second(usage.written_bytes, rates.elapsed, rates.baseline_ready),
        ));
    }
    let options = DiskOptions {
        use_fstab: false,
        ..options.clone()
    };
    super::macos::local_mounts()
        .into_iter()
        .filter(|mount| {
            mount_is_visible(&mount.mount, &mount.device, &mount.filesystem, &options, "")
        })
        .map(|mount| {
            let rates = super::macos::whole_disk(&mount.device).and_then(|disk| io.get(disk));
            DiskSnapshot {
                device: mount.device,
                mount: mount.mount,
                total_bytes: mount.total_bytes,
                used_bytes: mount.total_bytes.saturating_sub(mount.available_bytes),
                read_bytes_per_second: rates.map_or(0.0, |rates| rates.0),
                write_bytes_per_second: rates.map_or(0.0, |rates| rates.1),
                busy_percent: None,
                has_io: rates.is_some(),
            }
        })
        .collect()
}
