//! The folded snapshot plus its change-notification channel.
//!
//! Split out from the runtime so the render contract — snapshot in, ping out —
//! can be exercised without booting a core. The core uses process globals (a
//! `OnceLock` context, a singleton event bus) and cannot be stood up per test,
//! so anything that genuinely needs one belongs in an integration test. Keeping
//! this piece core-free is what makes the contract unit-testable at all.

use std::sync::Mutex;

use tokio::sync::broadcast;

use crate::runtime::types::{AgentDescriptor, RuntimeSnapshot, ThreadSummary};

/// Holds the folded view and notifies readers when it changes.
pub struct SnapshotCell {
    /// A plain mutex, not an async one: every write is a whole-snapshot swap,
    /// so the lock is never held across an await and there is no partial state
    /// for a reader to observe.
    state: Mutex<RuntimeSnapshot>,
    /// Payload-free ping. The contract is "something moved, re-read the
    /// snapshot", which keeps a slow reader from having to replay a backlog and
    /// makes a lagging receiver harmless.
    tx: broadcast::Sender<()>,
}

impl SnapshotCell {
    /// An empty cell.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(16);
        Self {
            state: Mutex::new(RuntimeSnapshot::default()),
            tx,
        }
    }

    /// The current folded view.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Subscribe to change notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Replace the roster and thread list, then notify.
    pub fn apply(&self, roster: Vec<AgentDescriptor>, threads: Vec<ThreadSummary>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.roster = roster;
            state.threads = threads;
        }
        // An absent or lagging receiver is not an error — the ping is advisory
        // and the next `snapshot()` reads the same state regardless.
        let _ = self.tx.send(());
    }

    /// Record the operator's active thread.
    pub fn set_active_thread(&self, id: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active_thread_id = id;
    }
}

impl Default for SnapshotCell {
    fn default() -> Self {
        Self::new()
    }
}
