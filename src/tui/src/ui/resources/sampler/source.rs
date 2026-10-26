//! The live, sysinfo-backed [`ResourceSource`].

use super::*;

/// Live source backed by one persistent sysinfo handle.
pub struct SystemResourceSource {
    system: System,
    disks: Disks,
    networks: Networks,
    last_sample: Option<Instant>,
    disk_peak_bytes_per_second: f64,
    cpu_monitor: crate::ui::resources::CpuMonitor,
    disk_options: DiskOptions,
    #[cfg(target_os = "linux")]
    linux: LinuxResourceMonitor,
}

impl Default for SystemResourceSource {
    fn default() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new(),
            networks: Networks::new_with_refreshed_list(),
            last_sample: None,
            disk_peak_bytes_per_second: 1.0,
            cpu_monitor: crate::ui::resources::CpuMonitor::default(),
            disk_options: DiskOptions::default(),
            #[cfg(target_os = "linux")]
            linux: LinuxResourceMonitor::default(),
        }
    }
}

impl SystemResourceSource {
    /// Set mount filtering used when building each resource snapshot.
    pub fn with_disk_options(mut self, options: DiskOptions) -> Self {
        self.disk_options = options;
        self
    }
    /// Keep the IO bar scaled to recent activity instead of only this sample.
    pub(super) fn update_disk_peak(&mut self, read_rate: f64, write_rate: f64) -> f64 {
        self.disk_peak_bytes_per_second = (self.disk_peak_bytes_per_second * 0.9)
            .max(read_rate)
            .max(write_rate)
            .max(1.0);
        self.disk_peak_bytes_per_second
    }
}

/// A counter delta as a rate over `elapsed` seconds.
///
/// Before the first baseline there is no interval: the delta covers however
/// long start-up took, so it is reported as no rate rather than as a spike.
pub(super) fn per_second(delta: u64, elapsed: f64, baseline_ready: bool) -> f64 {
    if baseline_ready {
        delta as f64 / elapsed.max(0.001)
    } else {
        0.0
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
        let process_refresh = {
            let refresh = ProcessRefreshKind::nothing();
            #[cfg(target_os = "linux")]
            {
                refresh
            }
            #[cfg(not(target_os = "linux"))]
            {
                refresh
                    .with_cpu()
                    .with_memory()
                    .with_disk_usage()
                    .with_exe(UpdateKind::OnlyIfNotSet)
                    .with_cmd(UpdateKind::OnlyIfNotSet)
                    .with_cwd(UpdateKind::OnlyIfNotSet)
            }
        };
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh);
        #[cfg(target_os = "linux")]
        let process_metric_pids = {
            let possible_agent_pids: Vec<_> = self
                .system
                .processes()
                .iter()
                .filter(|(_, process)| may_be_agent_process(&process.name().to_string_lossy()))
                .map(|(pid, _)| *pid)
                .collect();
            if !possible_agent_pids.is_empty() {
                self.system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&possible_agent_pids),
                    true,
                    ProcessRefreshKind::nothing()
                        .with_exe(UpdateKind::OnlyIfNotSet)
                        .with_cmd(UpdateKind::OnlyIfNotSet)
                        .with_cwd(UpdateKind::OnlyIfNotSet),
                );
            }
            let agent_pids: Vec<_> = self
                .system
                .processes()
                .iter()
                .filter(|(_, process)| should_refresh_task_details(process.exe(), process.cmd()))
                .map(|(pid, _)| *pid)
                .collect();
            let agent_tree_roots: Vec<_> = self
                .system
                .processes()
                .iter()
                .filter(|(_, process)| should_refresh_process_tree(process.exe(), process.cmd()))
                .map(|(pid, _)| *pid)
                .collect();
            let mut process_metric_pids = pids_in_process_trees(
                self.system
                    .processes()
                    .iter()
                    .map(|(pid, process)| (*pid, process.parent())),
                agent_tree_roots,
            );
            process_metric_pids.extend(agent_pids.iter().copied());
            if let Ok(current_pid) = sysinfo::get_current_pid() {
                process_metric_pids.push(current_pid);
            }
            process_metric_pids.sort_unstable();
            process_metric_pids.dedup();
            if !process_metric_pids.is_empty() {
                self.system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&process_metric_pids),
                    true,
                    ProcessRefreshKind::nothing()
                        .with_cpu()
                        .with_memory()
                        .with_disk_usage(),
                );
            }
            if !agent_pids.is_empty() {
                self.system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&agent_pids),
                    true,
                    ProcessRefreshKind::nothing().with_tasks(),
                );
            }
            process_metric_pids
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        };
        #[cfg(not(target_os = "linux"))]
        let process_metric_pids = self
            .system
            .processes()
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        self.disks.refresh(true);
        self.networks.refresh(true);
        let networks = self
            .networks
            .iter()
            .map(|(name, data)| {
                let ip_addresses: Vec<_> =
                    data.ip_networks().iter().map(ToString::to_string).collect();
                #[cfg(target_os = "linux")]
                let (up, running) = linux_interface_flags(name)
                    .map(linux_interface_state)
                    .unwrap_or((false, false));
                // sysinfo does not expose interface flags on other platforms.
                // An assigned address is evidence of a configured interface;
                // observed traffic is evidence it is currently connected.
                #[cfg(not(target_os = "linux"))]
                let (up, running) = (!ip_addresses.is_empty(), false);
                let received_bytes_per_second =
                    per_second(data.received(), elapsed, baseline_ready);
                let transmitted_bytes_per_second =
                    per_second(data.transmitted(), elapsed, baseline_ready);
                NetworkSnapshot {
                    interface: name.clone(),
                    up,
                    connected: running
                        || received_bytes_per_second > 0.0
                        || transmitted_bytes_per_second > 0.0,
                    ip_addresses,
                    received_bytes_per_second,
                    transmitted_bytes_per_second,
                    total_received_bytes: data.total_received(),
                    total_transmitted_bytes: data.total_transmitted(),
                }
            })
            .collect::<Vec<_>>();
        let mut networks = networks;
        networks.sort_by(|left, right| left.interface.cmp(&right.interface));
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
            .map(|(pid, process)| {
                let has_metrics = process_metric_pids.contains(pid);
                ProcessSnapshot {
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
                    owner_uid: effective_user_id(process),
                    cpu_percent: if has_metrics {
                        f64::from(process.cpu_usage())
                    } else {
                        0.0
                    },
                    memory_bytes: if has_metrics { process.memory() } else { 0 },
                    disk_read_bytes: if baseline_ready && has_metrics {
                        process.disk_usage().read_bytes
                    } else {
                        0
                    },
                    disk_written_bytes: if baseline_ready && has_metrics {
                        process.disk_usage().written_bytes
                    } else {
                        0
                    },
                    threads: process
                        .tasks()
                        .map(|tasks| tasks.len())
                        .or_else(|| thread_count(pid.as_u32()))
                        .unwrap_or(0),
                }
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
            Some(crate::ui::resources::ResourceSnapshot {
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
                .map(|cpu| crate::ui::resources::CpuReading {
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
            disks: disk_snapshots(
                &self.disks,
                &self.disk_options,
                #[cfg(not(target_os = "linux"))]
                DiskRates {
                    elapsed,
                    baseline_ready,
                },
                #[cfg(target_os = "linux")]
                &linux.io,
            ),
            networks,
            processes,
        }
    }

    fn update_disk_options(&mut self, options: DiskOptions) {
        self.disk_options = options;
    }
}

/// Thread count where sysinfo has none to give: it lists a process's threads
/// only on Linux, so macOS asks the kernel for the task's own count.
#[cfg(target_os = "macos")]
fn thread_count(pid: u32) -> Option<usize> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: `info` is a writable buffer of exactly `size` bytes, which is
    // what PROC_PIDTASKINFO fills; a short return means it was not filled.
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: the kernel wrote the whole struct, checked above.
    let info = unsafe { info.assume_init() };
    usize::try_from(info.pti_threadnum).ok()
}

#[cfg(not(target_os = "macos"))]
fn thread_count(_pid: u32) -> Option<usize> {
    None
}

pub(super) fn effective_user_id(process: &sysinfo::Process) -> Option<String> {
    #[cfg(unix)]
    {
        process.effective_user_id().map(|uid| (**uid).to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = process;
        None
    }
}
