//! Unit tests for the embedded-core runtime.
//!
//! These exercise the pure fold and the snapshot/notify contract, neither of
//! which needs a core. Anything that genuinely needs one belongs in an
//! integration test: the core uses process globals (a `OnceLock` context, a
//! singleton event bus), so it cannot be stood up and torn down per test.

use openhuman_core::embed::{RosterWorker, SessionSummary};

use super::cell::SnapshotCell;
use super::fold;
use crate::runtime::types::{AgentDescriptor, ThreadSummary};

fn thread(id: &str) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        name: String::new(),
        running: false,
        turns: 0,
        running_tasks: 0,
        attention: 0,
    }
}

fn worker(id: &str, label: &str) -> RosterWorker {
    RosterWorker {
        registry_id: id.to_string(),
        label: label.to_string(),
        description: "desc".to_string(),
        availability: "available".to_string(),
        ..Default::default()
    }
}

#[test]
fn roster_fold_preserves_identity_and_order() {
    // Order is the render order, so a reordering fold would shuffle the UI.
    let folded = fold::roster(vec![worker("w1", "First"), worker("w2", "Second")]);
    assert_eq!(
        folded,
        vec![
            AgentDescriptor {
                id: "w1".into(),
                name: "First".into(),
                description: "desc".into(),
                availability: "available".into(),
                ..AgentDescriptor::default()
            },
            AgentDescriptor {
                id: "w2".into(),
                name: "Second".into(),
                description: "desc".into(),
                availability: "available".into(),
                ..AgentDescriptor::default()
            },
        ]
    );
}

#[test]
fn roster_fold_handles_an_empty_roster() {
    assert!(fold::roster(Vec::new()).is_empty());
}

#[test]
fn thread_fold_defaults_a_missing_title_to_empty_not_a_placeholder() {
    // A synthesized name like "(untitled)" would render as if the backend sent
    // it; empty lets the UI apply its own placeholder.
    let folded = fold::threads(vec![SessionSummary {
        session_id: "s1".into(),
        title: None,
        ..Default::default()
    }]);
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].id, "s1");
    assert_eq!(folded[0].name, "");
}

#[test]
fn thread_fold_leaves_activity_counters_at_zero() {
    // The session list carries no per-thread activity. Inventing a count would
    // render as real data and quietly mislead.
    let folded = fold::threads(vec![SessionSummary {
        session_id: "s1".into(),
        title: Some("Work".into()),
        ..Default::default()
    }]);
    assert_eq!(
        (
            folded[0].turns,
            folded[0].running_tasks,
            folded[0].attention
        ),
        (0, 0, 0)
    );
    assert!(!folded[0].running);
}

#[test]
fn cell_starts_empty() {
    let cell = SnapshotCell::new();
    let snap = cell.snapshot();
    assert!(snap.roster.is_empty());
    assert!(snap.threads.is_empty());
}

#[test]
fn apply_replaces_rather_than_appends() {
    // Two refreshes must not accumulate: a roster that only grows would show
    // workers that have since disconnected.
    let cell = SnapshotCell::new();
    let one = vec![AgentDescriptor {
        id: "w1".into(),
        name: "First".into(),
        description: String::new(),
        availability: "available".into(),
        ..AgentDescriptor::default()
    }];
    cell.apply(one.clone(), Vec::new());
    cell.apply(one, Vec::new());
    assert_eq!(cell.snapshot().roster.len(), 1);
}

#[tokio::test]
async fn apply_notifies_subscribers() {
    let cell = SnapshotCell::new();
    let mut rx = cell.subscribe();
    cell.apply(Vec::new(), vec![thread("t1")]);
    assert!(rx.recv().await.is_ok(), "subscriber must be pinged");
    assert_eq!(cell.snapshot().threads.len(), 1);
}

#[test]
fn apply_without_subscribers_is_not_an_error() {
    // The ping is advisory; nobody listening is the normal case at startup.
    let cell = SnapshotCell::new();
    cell.apply(Vec::new(), Vec::new());
    assert!(cell.snapshot().roster.is_empty());
}

#[test]
fn active_thread_survives_a_refresh() {
    // The operator's selection is theirs, not the backend's — a refresh that
    // reset it would move the UI out from under them.
    let cell = SnapshotCell::new();
    cell.set_active_thread("thread-7".into());
    cell.apply(Vec::new(), vec![thread("t1")]);
    assert_eq!(cell.snapshot().active_thread_id, "thread-7");
}
