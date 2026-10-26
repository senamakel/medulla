use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use super::{
    spawn_sampler, spawn_sampler_with_disk_options, update_disk_options, update_interval,
    DiskOptions, DiskSnapshot, History, ResourceSnapshot, ResourceSource,
};

#[derive(Clone)]
struct SequenceSource(Arc<AtomicUsize>);

impl ResourceSource for SequenceSource {
    fn sample(&mut self) -> ResourceSnapshot {
        ResourceSnapshot {
            cpu_percent: self.0.fetch_add(1, Ordering::Relaxed) as f64,
            ..Default::default()
        }
    }
}

#[tokio::test]
async fn sampler_posts_injected_samples_in_order() {
    let (interval_tx, interval_rx) = watch::channel(Some(Duration::from_millis(5)));
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let task = spawn_sampler(
        SequenceSource(Arc::new(AtomicUsize::new(0))),
        interval_rx,
        2,
        move |snapshot| output_tx.send(snapshot).is_ok(),
    );
    let first = tokio::time::timeout(Duration::from_secs(1), output_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(1), output_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.cpu_percent, 0.0);
    assert_eq!(second.cpu_percent, 1.0);
    assert_eq!(
        second
            .history
            .cpu_percent
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0.0, 1.0]
    );
    drop(interval_tx);
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn changed_interval_controls_the_next_sample_tick() {
    let (interval_tx, interval_rx) = watch::channel(Some(Duration::from_secs(5)));
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let task = spawn_sampler(
        SequenceSource(Arc::new(AtomicUsize::new(0))),
        interval_rx,
        2,
        move |snapshot| output_tx.send(snapshot).is_ok(),
    );

    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_secs(4)).await;
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    assert!(output_rx.try_recv().is_err());

    interval_tx.send(Some(Duration::from_millis(20))).unwrap();
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_millis(19)).await;
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    assert!(output_rx.try_recv().is_err());
    tokio::time::advance(Duration::from_millis(1)).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
        if output_rx.try_recv().is_ok() {
            task.abort();
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    task.abort();
    panic!("updated resource interval did not produce a sample");
}

#[tokio::test(start_paused = true)]
async fn repeated_unchanged_interval_updates_do_not_starve_samples() {
    let period = Duration::from_millis(20);
    let (interval_tx, interval_rx) = watch::channel(Some(period));
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let task = spawn_sampler(
        SequenceSource(Arc::new(AtomicUsize::new(0))),
        interval_rx,
        2,
        move |snapshot| output_tx.send(snapshot).is_ok(),
    );
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    for _ in 0..5 {
        assert!(!update_interval(&interval_tx, Some(period)));
        tokio::task::yield_now().await;
    }
    tokio::time::advance(period).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
        if output_rx.try_recv().is_ok() {
            task.abort();
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    task.abort();
    panic!("unchanged interval updates starved the scheduled sample");
}

#[derive(Clone)]
struct DiskOptionsSource(Arc<std::sync::Mutex<DiskOptions>>);

impl ResourceSource for DiskOptionsSource {
    fn sample(&mut self) -> ResourceSnapshot {
        let options = self.0.lock().unwrap().clone();
        let disks = [
            ("loop0", "/boot", false, false),
            ("sda", "/", true, true),
            ("sdb", "/boot", true, true),
            ("sdc", "/data", true, false),
        ]
        .into_iter()
        .filter(|(device, mount, physical, in_fstab)| {
            (!options.only_physical || *physical)
                && (!options.use_fstab || *in_fstab)
                && !options
                    .filter
                    .split(',')
                    .filter_map(|filter| filter.strip_prefix("exclude="))
                    .any(|excluded| excluded == *mount)
                && (*device != "loop0" || !options.only_physical)
        })
        .map(|(device, mount, _, _)| DiskSnapshot {
            device: device.into(),
            mount: mount.into(),
            total_bytes: 0,
            used_bytes: 0,
            read_bytes_per_second: 0.0,
            write_bytes_per_second: 0.0,
            busy_percent: None,
            has_io: true,
        })
        .collect();
        ResourceSnapshot {
            disks,
            ..Default::default()
        }
    }

    fn update_disk_options(&mut self, options: DiskOptions) {
        *self.0.lock().unwrap() = options;
    }
}

#[tokio::test]
async fn disk_options_change_samples_without_restarting_the_sampler() {
    let initial = DiskOptions::default();
    let (options_tx, options_rx) = watch::channel(initial.clone());
    let (interval_tx, interval_rx) = watch::channel(Some(Duration::from_millis(5)));
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let task = spawn_sampler_with_disk_options(
        DiskOptionsSource(Arc::new(std::sync::Mutex::new(initial))),
        interval_rx,
        options_rx,
        2,
        move |snapshot| output_tx.send(snapshot).is_ok(),
    );

    async fn sample_matching(
        output_rx: &mut mpsc::UnboundedReceiver<Arc<ResourceSnapshot>>,
        matches: impl Fn(&ResourceSnapshot) -> bool,
    ) -> Arc<ResourceSnapshot> {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let snapshot = output_rx.recv().await.expect("sampler should stay alive");
                if matches(&snapshot) {
                    return snapshot;
                }
            }
        })
        .await
        .expect("sampler should publish a matching snapshot within one second")
    }

    assert_eq!(
        sample_matching(&mut output_rx, |snapshot| snapshot.disks.len() == 4)
            .await
            .disks
            .len(),
        4
    );
    assert!(update_disk_options(
        &options_tx,
        DiskOptions {
            only_physical: true,
            ..Default::default()
        }
    ));
    assert_eq!(
        sample_matching(&mut output_rx, |snapshot| snapshot.disks.len() == 3)
            .await
            .disks
            .len(),
        3
    );
    assert!(update_disk_options(
        &options_tx,
        DiskOptions {
            only_physical: true,
            use_fstab: true,
            ..Default::default()
        }
    ));
    assert_eq!(
        sample_matching(&mut output_rx, |snapshot| snapshot.disks.len() == 2)
            .await
            .disks
            .len(),
        2
    );
    assert!(update_disk_options(
        &options_tx,
        DiskOptions {
            only_physical: true,
            use_fstab: true,
            filter: "exclude=/boot".into(),
        }
    ));
    let final_sample = sample_matching(&mut output_rx, |snapshot| {
        snapshot.disks.len() == 1 && snapshot.disks[0].mount == "/"
    })
    .await;
    assert_eq!(final_sample.disks.len(), 1);
    assert_eq!(final_sample.disks[0].mount, "/");

    drop(interval_tx);
    task.abort();
}

#[test]
fn history_trims_to_requested_capacity_and_clones_cheaply() {
    let mut history = History::new(3);
    history.push(1, 2);
    history.push(2, 2);
    let clone = history.clone();
    history.push(3, 2);
    assert_eq!(history.iter().copied().collect::<Vec<_>>(), [2, 3]);
    assert_eq!(clone.iter().copied().collect::<Vec<_>>(), [1, 2]);
    history.push(4, 0);
    assert!(history.is_empty());
}

#[test]
fn resource_history_keeps_each_memory_series_for_graphs() {
    let sample = ResourceSnapshot {
        memory_used_bytes: 10,
        memory_available_bytes: 20,
        memory_cached_bytes: 30,
        memory_free_bytes: 40,
        swap_used_bytes: 50,
        ..Default::default()
    };
    let mut history = super::ResourceHistory::default();
    history.push(&sample, 8);

    assert_eq!(
        history
            .memory_used_bytes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [10]
    );
    assert_eq!(
        history
            .memory_available_bytes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [20]
    );
    assert_eq!(
        history
            .memory_cached_bytes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [30]
    );
    assert_eq!(
        history
            .memory_free_bytes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [40]
    );
    assert_eq!(
        history.swap_used_bytes.iter().copied().collect::<Vec<_>>(),
        [50]
    );
}

#[test]
fn process_io_peak_decays_instead_of_resetting_to_each_sample() {
    let mut source = super::SystemResourceSource::default();
    assert_eq!(source.update_disk_peak(100.0, 0.0), 100.0);
    assert_eq!(source.update_disk_peak(0.0, 0.0), 90.0);
}

#[test]
fn the_first_sample_reports_no_rate_instead_of_a_startup_spike() {
    // Bytes counted between start-up and the first sample have no interval
    // to divide by; reporting them over the 1 ms floor made a gigabit spike
    // that pinned the network scale and its Top reading.
    assert_eq!(super::per_second(4_096, 0.0, false), 0.0);
    assert_eq!(super::per_second(4_096, 2.0, true), 2_048.0);
    assert_eq!(super::per_second(4_096, 0.0, true), 4_096_000.0);
}

mod filters;

/// The agents table reads a process's working directory and thread count from
/// the sample; on macOS neither used to be collected, so every row read `—`
/// and `0`. Linux reads both only for processes that may be agents, which the
/// test runner is not.
#[cfg(not(target_os = "linux"))]
#[test]
fn a_sample_carries_this_processs_working_directory_and_threads() {
    let mut source = super::SystemResourceSource::default();
    let snapshot = source.sample();
    let me = snapshot
        .processes
        .iter()
        .find(|process| process.pid == std::process::id())
        .expect("the sampler lists its own process");
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(
        me.cwd.as_deref().map(std::path::Path::new),
        Some(cwd.as_path()),
        "{me:?}"
    );
    #[cfg(target_os = "macos")]
    assert!(me.threads >= 1, "{me:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_volumes_resolve_to_the_disk_they_live_on() {
    use super::macos::whole_disk;
    assert_eq!(whole_disk("/dev/disk3s1s1"), Some("disk3"));
    assert_eq!(whole_disk("/dev/disk12s2"), Some("disk12"));
    assert_eq!(whole_disk("devfs"), None);
    assert_eq!(whole_disk("map auto_home"), None);
}

/// sysinfo lists only the volumes Finder browses; the mount table has every
/// local one, as btop shows, without the pseudo filesystems.
#[cfg(target_os = "macos")]
#[test]
fn macos_local_mounts_include_root_and_skip_pseudo_filesystems() {
    let mounts = super::macos::local_mounts();
    assert!(mounts.iter().any(|mount| mount.mount == "/"), "no root");
    assert!(mounts
        .iter()
        .all(|mount| !["devfs", "autofs"].contains(&mount.filesystem.as_str())));
    assert!(mounts
        .iter()
        .all(|mount| mount.total_bytes > 0 || mount.mount != "/"));
}
