//! [`Runtime`] backed by the embedded OpenHuman core.
//!
//! A fourth [`Runtime`] implementation alongside `backend` (HTTP/SSE), `core`
//! (NDJSON socket) and `mock`. It is the one this migration is heading toward:
//! rather than speaking to a remote process, it drives an OpenHuman core
//! running in-process through the typed embed facade.
//!
//! # Why a `Runtime` impl rather than replacing the trait
//!
//! [`Runtime`] is not "the backend abstraction" — it is the *render-snapshot*
//! contract. Every surface depends on two things,
//! [`snapshot`](Runtime::snapshot) and [`subscribe`](Runtime::subscribe), and
//! [`RuntimeSnapshot`] is a fold of an event stream. That fold and the code
//! rendering it do not care where the events came from. Keeping the trait is
//! also what keeps `MockRuntime` viable, and with it the TUI test suites that
//! drive the whole UI with no backend at all.
//!
//! # Layout
//!
//! [`fold`] is pure translation, [`cell`] is the snapshot plus its notification
//! channel, and this module is the thin part that needs a core. The split is
//! what lets the contract be unit-tested: the core uses process globals and
//! cannot be built per test.
//!
//! # Current scope
//!
//! Reads fold live roster and session data. Submit, abort and new-session drive
//! the backend through the facade. What remains unwired is chat-tree resume —
//! `medulla_chat` lives in the core but has no RPC surface yet — and the event
//! stream, so a submitted turn is accepted but its reply does not yet flow back
//! into the snapshot.
//!
//! The still-unwired paths return a typed error rather than doing nothing. A
//! silent no-op reads as a hung backend and sends someone debugging the
//! network; an explicit error names the layer that is actually missing.

use std::sync::Arc;

use futures::future::BoxFuture;
use openhuman_core::embed::{Core, CoreError};
use tokio::sync::broadcast;

use super::types::{ContextItem, RuntimeSnapshot, StreamState};

/// Poll delay while a turn is actively producing events.
const POLL_ACTIVE: std::time::Duration = std::time::Duration::from_millis(120);

/// Ceiling the poll delay backs off to once a session goes quiet.
const POLL_IDLE: std::time::Duration = std::time::Duration::from_millis(1_000);
use super::Runtime;
use crate::ui::chat_store::MainChatSummary;

pub mod cell;
pub mod fold;

#[cfg(test)]
mod tests;

pub use cell::SnapshotCell;

/// Message for every drive method not yet migrated.
///
/// One constant so the wording cannot drift between call sites, and so tests
/// assert a class of failure rather than prose.
pub const NOT_YET_WIRED: &str =
    "this action is not wired to the embedded core yet (read-only while the migration lands)";

/// A [`Runtime`] driving an in-process OpenHuman core.
pub struct OpenHumanRuntime {
    core: Arc<Core>,
    cell: SnapshotCell,
    /// The session `submit`/`abort` act on.
    ///
    /// Minted lazily on first submit rather than at construction: booting must
    /// not create a durable session on the backend for a host that only ever
    /// reads, and a failed boot should leave no trace behind.
    session: SessionSlot,
    /// Replay cursor: the highest event `seq` already folded in.
    ///
    /// Shared so the poll loop can advance it from its own task. Starts at
    /// `None`, which the backend reads as "replay from the beginning".
    cursor: Arc<tokio::sync::Mutex<Option<i64>>>,
}

/// Shared handle to the active session id.
///
/// An `Arc` because `submit` and `abort` move it into `'static` futures — the
/// trait's futures outlive the borrow of `&self`.
type SessionSlot = Arc<tokio::sync::Mutex<Option<String>>>;

impl OpenHumanRuntime {
    /// Wrap an already-booted core.
    ///
    /// Performs no I/O — call [`refresh`](Self::refresh) for that. Boot and
    /// first fetch are separate so a host can paint its UI before the first
    /// round trip lands.
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            core,
            cell: SnapshotCell::new(),
            session: SessionSlot::default(),
            cursor: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// A clone of the shared session slot, for moving into a `'static` future.
    fn session_handle(&self) -> SessionSlot {
        Arc::clone(&self.session)
    }

    /// The active session, minting one if there is none yet.
    ///
    /// The lock is held across the mint so two concurrent submits cannot each
    /// create a session and leave one orphaned on the backend. Static rather
    /// than a method because the trait's futures are `'static` and cannot
    /// borrow `&self`.
    async fn session_for(core: &Core, slot: &SessionSlot) -> Result<String, CoreError> {
        let mut guard = slot.lock().await;
        if let Some(id) = guard.as_ref() {
            return Ok(id.clone());
        }
        let created = core.medulla().create_session(None).await?;
        tracing::debug!("[openhuman_runtime] minted session {}", created.session_id);
        *guard = Some(created.session_id.clone());
        Ok(created.session_id)
    }

    /// Pull the roster and session list, fold them in, and notify.
    ///
    /// Best-effort by design: an unconfigured or signed-out host is an expected
    /// state rather than a failure, so a rejected call leaves the previous
    /// snapshot intact instead of blanking the UI.
    pub async fn refresh(&self) {
        let medulla = self.core.medulla();

        let roster = match medulla.roster().await {
            Ok(workers) => fold::roster(workers),
            Err(err) => {
                tracing::debug!("[openhuman_runtime] roster unavailable: {err}");
                Vec::new()
            }
        };
        let threads = match medulla.list_sessions().await {
            Ok(sessions) => fold::threads(sessions),
            Err(err) => {
                tracing::debug!("[openhuman_runtime] sessions unavailable: {err}");
                Vec::new()
            }
        };

        tracing::debug!(
            "[openhuman_runtime] refresh roster={} threads={}",
            roster.len(),
            threads.len()
        );
        self.cell.apply(roster, threads);
    }

    /// Fetch events past the cursor and fold them into the snapshot.
    ///
    /// Returns the number folded, so a caller can back off when a stream is
    /// idle instead of polling at a fixed rate regardless of traffic.
    ///
    /// Best-effort like [`refresh`](Self::refresh): a rejected fetch leaves the
    /// snapshot and the cursor untouched rather than blanking the transcript or
    /// replaying it from the start.
    pub async fn poll_events(&self) -> usize {
        let Some(session) = self.session.lock().await.clone() else {
            return 0;
        };
        let after = *self.cursor.lock().await;

        let fetched = match self.core.medulla().list_events(&session, after).await {
            Ok(events) => events,
            Err(err) => {
                tracing::debug!("[openhuman_runtime] event poll failed: {err}");
                return 0;
            }
        };

        let events = fold::events(fetched);
        if events.is_empty() {
            return 0;
        }

        // Advance the cursor BEFORE folding, so a panic mid-fold cannot leave
        // the cursor behind and replay the same batch forever.
        if let Some(max) = fold::max_seq(&events) {
            *self.cursor.lock().await = Some(max as i64);
        }
        let count = events.len();
        tracing::debug!("[openhuman_runtime] folded {count} events after {after:?}");
        // `running` stays true here: a batch arriving means the turn is still
        // producing. The settle signal comes from the session detail, not the
        // event log, so claiming settled here would flicker the spinner off
        // between batches.
        self.cell.append_events(events, true);
        count
    }

    /// Drive [`poll_events`](Self::poll_events) in the background until dropped.
    ///
    /// Returns immediately; the caller keeps the returned handle only if it
    /// wants to stop the loop early. Takes `Arc<Self>` because the loop outlives
    /// the call and the runtime is already shared with the UI.
    ///
    /// The cadence adapts: a batch means the turn is producing, so poll again
    /// promptly; an empty fetch backs off toward [`POLL_IDLE`]. A fixed fast
    /// tick would spend most of its life querying an idle session, and a fixed
    /// slow one would make streaming replies arrive in visible steps.
    pub fn spawn_poll_loop(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let rt = Arc::clone(self);
        tokio::spawn(async move {
            let mut delay = POLL_IDLE;
            loop {
                tokio::time::sleep(delay).await;
                delay = if rt.poll_events().await > 0 {
                    POLL_ACTIVE
                } else {
                    // Ease back rather than snapping to idle: a turn often has
                    // gaps between batches, and snapping would add the full
                    // idle delay to the very next token.
                    (delay * 2).min(POLL_IDLE)
                };
            }
        })
    }

    /// The facade, for callers needing something the trait does not model.
    pub fn core(&self) -> &Arc<Core> {
        &self.core
    }
}

impl Runtime for OpenHumanRuntime {
    fn describe(&self) -> String {
        "openhuman (embedded core)".to_string()
    }

    fn snapshot(&self) -> RuntimeSnapshot {
        self.cell.snapshot()
    }

    fn subscribe(&self) -> broadcast::Receiver<()> {
        self.cell.subscribe()
    }

    fn submit(&self, input: String) -> BoxFuture<'static, anyhow::Result<()>> {
        let core = Arc::clone(&self.core);
        let session = self.session_handle();
        Box::pin(async move {
            let id = Self::session_for(&core, &session).await?;
            // Non-blocking: the reply arrives over the event stream, so the UI
            // can render progress instead of freezing until the turn finishes.
            core.medulla().send_message(&id, &input, false).await?;
            Ok(())
        })
    }

    fn stream_state(&self) -> Option<StreamState> {
        // Polled replay rather than a live stream, so "Live" would overstate
        // it. Reporting the honest state keeps the header from claiming a
        // push connection this runtime does not hold.
        Some(StreamState::Resyncing)
    }

    fn abort(&self) {
        let core = Arc::clone(&self.core);
        let session = self.session_handle();
        // Fire-and-forget: the trait is sync, and an abort that has not landed
        // yet is still better than blocking the UI thread on a round trip.
        tokio::spawn(async move {
            let Some(id) = session.lock().await.clone() else {
                tracing::debug!("[openhuman_runtime] abort with no active session");
                return;
            };
            if let Err(err) = core.medulla().abort(&id).await {
                tracing::debug!("[openhuman_runtime] abort failed: {err}");
            }
        });
    }

    fn new_session(&self) {
        let session = self.session_handle();
        // Clear rather than mint: the next submit mints one. Minting here would
        // create a durable session the operator may never use.
        tokio::spawn(async move {
            *session.lock().await = None;
            tracing::debug!("[openhuman_runtime] active session cleared");
        });
    }

    fn set_active_thread(&self, id: String) {
        self.cell.set_active_thread(id);
    }

    fn list_main_chats(&self) -> BoxFuture<'static, anyhow::Result<Vec<MainChatSummary>>> {
        // The chat-tree store lives in the core as `medulla_chat` but has no RPC
        // surface yet. Empty rather than an error: "no saved chats" is a state
        // the Chat tab already renders, whereas an error would show a failure
        // where there is none.
        Box::pin(async { Ok(Vec::new()) })
    }

    fn resume_chat(&self, _main_session_id: String) -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async { Err(anyhow::anyhow!(NOT_YET_WIRED)) })
    }

    fn inspect_context(&self) -> BoxFuture<'static, anyhow::Result<Vec<ContextItem>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn shutdown(&self) -> BoxFuture<'static, anyhow::Result<()>> {
        // The embedder owns the core's lifecycle; this runtime borrows it.
        // Tearing it down here would leave the process without a core and no
        // way to build a second one — the context is a `OnceLock`, so a relogin
        // could never recover.
        Box::pin(async { Ok(()) })
    }
}
