//! Which processes get detailed refreshes, which disks are listed, and the
//! Linux `/proc` and `/sys` readings.

#[cfg(target_os = "linux")]
use super::*;

#[cfg(target_os = "linux")]
#[test]
fn process_snapshots_skip_executable_and_command_for_unclassified_processes() {
    let mut source = super::super::SystemResourceSource::default();
    let snapshot = source.sample();
    let pid = std::process::id();
    let process = snapshot
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .unwrap_or_else(|| panic!("sampler omitted its own process {pid}"));

    assert!(process.exe.is_none());
    assert!(process.cmd.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn process_snapshots_include_executable_and_command_for_agent_candidate_children() {
    let mut child = std::process::Command::new("bash")
        .args(["-c", "read -r line"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let pid = child.id();
    let snapshot = super::super::SystemResourceSource::default().sample();
    let process = snapshot
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .cloned();
    let _ = child.kill();
    let _ = child.wait();

    let process = process.unwrap_or_else(|| panic!("sampler omitted external fixture PID {pid}"));
    assert!(
        process.exe.as_deref().is_some_and(|exe| {
            std::path::Path::new(exe)
                .file_name()
                .is_some_and(|name| name == "bash")
        }),
        "agent candidate snapshot must include its executable: {process:?}"
    );
    assert!(
        process
            .cmd
            .iter()
            .any(|argument| argument == "read -r line"),
        "agent candidate snapshot must include argv: {process:?}"
    );
    drop(stdin);
}

#[cfg(target_os = "linux")]
#[test]
fn thread_details_are_refreshed_only_for_agent_candidates() {
    use std::ffi::OsString;
    use std::path::Path;

    assert!(super::super::should_refresh_task_details(
        Some(Path::new("/usr/local/bin/claude")),
        &[OsString::from("claude")],
    ));
    assert!(super::super::should_refresh_task_details(
        Some(Path::new("/usr/bin/node")),
        &[
            OsString::from("node"),
            OsString::from("/opt/@agentclientprotocol/claude-agent-acp"),
        ],
    ));
    assert!(!super::super::should_refresh_task_details(
        Some(Path::new("/usr/bin/sleep")),
        &[OsString::from("sleep"), OsString::from("120")],
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn process_details_are_refreshed_only_for_possible_agent_processes() {
    assert!(super::super::may_be_agent_process("claude"));
    assert!(super::super::may_be_agent_process("node"));
    assert!(super::super::may_be_agent_process("bash"));
    assert!(!super::super::may_be_agent_process("sleep"));
    assert!(!super::super::may_be_agent_process("postgres"));
}

#[cfg(target_os = "linux")]
#[test]
fn agent_metric_refresh_includes_descendants_without_refreshing_unrelated_trees() {
    use std::collections::HashSet;
    use sysinfo::Pid;

    let processes = [
        (Pid::from_u32(10), None),
        (Pid::from_u32(11), Some(Pid::from_u32(10))),
        (Pid::from_u32(12), Some(Pid::from_u32(11))),
        (Pid::from_u32(20), None),
        (Pid::from_u32(21), Some(Pid::from_u32(20))),
    ];
    let refreshed = super::super::pids_in_process_trees(processes, [Pid::from_u32(10)])
        .into_iter()
        .collect::<HashSet<_>>();

    assert_eq!(
        refreshed,
        [Pid::from_u32(10), Pid::from_u32(11), Pid::from_u32(12)]
            .into_iter()
            .collect()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn shell_candidates_do_not_make_every_child_an_agent_metric() {
    use std::ffi::OsString;
    use std::path::Path;

    assert!(!super::super::should_refresh_process_tree(
        Some(Path::new("/usr/bin/bash")),
        &[
            OsString::from("bash"),
            OsString::from("/app/tests_resources.sh")
        ],
    ));
    assert!(super::super::should_refresh_process_tree(
        Some(Path::new("/usr/local/bin/codex")),
        &[OsString::from("codex")],
    ));
}

#[test]
fn device_disk_selection_prefers_the_deepest_mount_containing_cwd() {
    let disks = [
        super::super::DiskSnapshot {
            device: "sda1".into(),
            mount: "/".into(),
            total_bytes: 10_000,
            used_bytes: 9_000,
            read_bytes_per_second: 0.0,
            write_bytes_per_second: 0.0,
            busy_percent: None,
        },
        super::super::DiskSnapshot {
            device: "sdb1".into(),
            mount: "/mnt/data".into(),
            total_bytes: 1_000,
            used_bytes: 250,
            read_bytes_per_second: 0.0,
            write_bytes_per_second: 0.0,
            busy_percent: None,
        },
    ];

    let selected =
        super::super::types::most_specific_mount(&disks, std::path::Path::new("/mnt/data/project"))
            .unwrap();
    assert_eq!(selected.mount, "/mnt/data");
}

#[test]
fn disk_filters_exclude_mount_subtrees_and_virtual_filesystems() {
    let options = super::super::DiskOptions {
        filter: "exclude=/boot".into(),
        ..Default::default()
    };
    assert!(!super::super::mount_is_visible(
        "/boot",
        "/dev/sda1",
        "ext4",
        &options,
        ""
    ));
    assert!(!super::super::mount_is_visible(
        "/boot/efi",
        "/dev/sda2",
        "vfat",
        &options,
        ""
    ));
    assert!(super::super::mount_is_visible(
        "/",
        "/dev/sda3",
        "ext4",
        &options,
        ""
    ));
    let physical = super::super::DiskOptions {
        only_physical: true,
        ..Default::default()
    };
    assert!(!super::super::mount_is_visible(
        "/run", "tmpfs", "tmpfs", &physical, ""
    ));
    assert!(!super::super::mount_is_visible(
        "/app", "overlay", "overlay", &physical, ""
    ));
    assert!(super::super::mount_is_visible(
        "/",
        "/dev/sda3",
        "ext4",
        &physical,
        ""
    ));
}

#[test]
fn fstab_mode_keeps_only_configured_mounts() {
    let options = super::super::DiskOptions {
        use_fstab: true,
        ..Default::default()
    };
    let fstab = "/dev/root / ext4 defaults 0 1\n/dev/data /mnt/data\\040drive ext4 defaults 0 2\n";
    assert!(super::super::mount_is_visible(
        "/",
        "/dev/root",
        "ext4",
        &options,
        fstab
    ));
    assert!(super::super::mount_is_visible(
        "/mnt/data drive",
        "/dev/data",
        "ext4",
        &options,
        fstab
    ));
    assert!(!super::super::mount_is_visible(
        "/var",
        "/dev/root",
        "ext4",
        &options,
        fstab
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_meminfo_reports_available_and_cached_with_legacy_fallback() {
    let memory = super::super::parse_memory("MemTotal: 1000 kB\nMemFree: 100 kB\nCached: 300 kB\nMemAvailable: 700 kB\nSwapTotal: 50 kB\n");
    assert_eq!(
        (memory.total, memory.available, memory.cached),
        (1_024_000, 716_800, 307_200)
    );
    let fallback =
        super::super::parse_memory("MemTotal: 1000 kB\nMemFree: 100 kB\nCached: 300 kB\n");
    assert_eq!(fallback.available, 409_600);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_block_stat_deltas_report_read_write_rates_and_busy_percent() {
    let before = super::super::parse_block_stat("8 0 100 500 8 0 200 900 0 300 1400").unwrap();
    let after = super::super::parse_block_stat("8 0 110 700 8 0 220 1300 0 450 2000").unwrap();
    let reading = super::super::io_delta(before, after, 2.0);
    assert_eq!(reading.read_rate, 2_560.0);
    assert_eq!(reading.write_rate, 5_120.0);
    assert_eq!(reading.busy, 7.5);
    assert_eq!(
        super::super::io_delta((0, 0, 0), (0, 0, 10_000), 1.0).busy,
        100.0
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_partition_block_stats_are_read_from_class_block() {
    let root = tempfile::tempdir().unwrap();
    let partition_stat = root.path().join("sys/class/block/nvme0n1p1/stat");
    std::fs::create_dir_all(partition_stat.parent().unwrap()).unwrap();
    std::fs::write(&partition_stat, "259 1 100 500 259 1 200 900 0 300 1400").unwrap();

    assert_eq!(
        super::super::read_block_stat(root.path(), "nvme0n1p1"),
        Some((100, 200, 300))
    );
}
