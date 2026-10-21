use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use super::{spawn_sampler, History, ResourceSnapshot, ResourceSource};

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
        move |snapshot| {
            let _ = output_tx.send(snapshot);
        },
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
    assert_eq!(second.history.cpu_percent.iter().copied().collect::<Vec<_>>(), [0.0, 1.0]);
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
        move |snapshot| {
            let _ = output_tx.send(snapshot);
        },
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
