//! Unit tests for `cpu`.

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

#[test]
fn missing_cpu_frequency_keeps_logical_cpu_slot() {
    let temp = tempfile::tempdir().unwrap();
    let cpu1 = temp.path().join("sys/devices/system/cpu/cpu1/cpufreq");
    fs::create_dir_all(&cpu1).unwrap();
    fs::write(cpu1.join("scaling_cur_freq"), "1200000\n").unwrap();

    assert_eq!(read_frequencies(temp.path()), [None, Some(1200)]);
}

#[test]
fn repeated_core_labels_map_to_distinct_packages() {
    let temp = tempfile::tempdir().unwrap();
    let write = |path: &str, value: &str| {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    };
    write("proc/cpuinfo", "processor : 0\nphysical id : 0\ncore id : 0\n\nprocessor : 1\nphysical id : 0\ncore id : 1\n\nprocessor : 2\nphysical id : 1\ncore id : 0\n\nprocessor : 3\nphysical id : 1\ncore id : 1\n");
    for (hwmon, package, temp_c) in [(0, 0, 51), (1, 1, 61)] {
        let device = temp
            .path()
            .join(format!("sys/devices/platform/coretemp.{hwmon}"));
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("physical_package_id"), package.to_string()).unwrap();
        let hwmon_path = temp.path().join(format!("sys/class/hwmon/hwmon{hwmon}"));
        fs::create_dir_all(&hwmon_path).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&device, hwmon_path.join("device")).unwrap();
        fs::write(hwmon_path.join("temp1_input"), format!("{}000\n", temp_c)).unwrap();
        fs::write(hwmon_path.join("temp1_label"), "Core 0\n").unwrap();
    }

    assert_eq!(
        read_temperatures(temp.path()).2,
        [(0, 51.0, 90.0), (2, 61.0, 90.0)]
    );
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
    write(
        "proc/cpuinfo",
        "processor : 0\nphysical id : 0\ncore id : 0\n",
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
