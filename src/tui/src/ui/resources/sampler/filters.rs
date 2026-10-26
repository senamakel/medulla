//! Which processes get detailed refreshes, and which disks are listed.

use super::*;

/// Classify a process from what sysinfo has read of it so far.
#[cfg(target_os = "linux")]
fn agent_kind(
    exe: Option<&std::path::Path>,
    cmd: &[std::ffi::OsString],
) -> Option<medulla::resources::agents::AgentKind> {
    medulla::resources::agents::classify_process(&medulla::resources::agents::ProcessRecord {
        pid: 0,
        ppid: None,
        exe: exe.map(std::path::Path::to_path_buf),
        argv: Some(
            cmd.iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        ),
        cwd: None,
        started_at_secs: 0,
        usage: Default::default(),
    })
}

#[cfg(target_os = "linux")]
pub(super) fn should_refresh_task_details(
    exe: Option<&std::path::Path>,
    cmd: &[std::ffi::OsString],
) -> bool {
    agent_kind(exe, cmd).is_some()
}

#[cfg(target_os = "linux")]
pub(super) fn should_refresh_process_tree(
    exe: Option<&std::path::Path>,
    cmd: &[std::ffi::OsString],
) -> bool {
    agent_kind(exe, cmd).is_some_and(|kind| kind != medulla::resources::agents::AgentKind::Shell)
}

#[cfg(target_os = "linux")]
pub(super) fn may_be_agent_process(name: &str) -> bool {
    medulla::resources::agents::may_be_agent_name(name)
}

#[cfg(target_os = "linux")]
pub(super) fn pids_in_process_trees(
    processes: impl IntoIterator<Item = (Pid, Option<Pid>)>,
    roots: impl IntoIterator<Item = Pid>,
) -> Vec<Pid> {
    let mut children = std::collections::HashMap::<Pid, Vec<Pid>>::new();
    for (pid, parent) in processes {
        if let Some(parent) = parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    if pending.is_empty() {
        return Vec::new();
    }
    let mut included = std::collections::HashSet::new();
    while let Some(pid) = pending.pop() {
        if !included.insert(pid) {
            continue;
        }
        if let Some(descendants) = children.get(&pid) {
            pending.extend(descendants.iter().copied());
        }
    }
    included.into_iter().collect()
}

pub(super) fn disk_is_visible(disk: &sysinfo::Disk, options: &DiskOptions) -> bool {
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

pub(super) fn mount_is_visible(
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
