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
//! # Current scope: reads are real, writes are not
//!
//! The read path folds live roster and session data from the core. The drive
//! path — submit, abort, resume — returns a typed not-yet-migrated error rather
//! than doing nothing. A silent no-op `submit` reads as a hung backend and
//! sends someone debugging the network; an explicit error names the layer that
//! is actually missing. These land as the core's Medulla RPC surface grows
//! beyond status/sessions/roster.

use std::sync::Arc;

use futures::future::BoxFuture;
use openhuman_core::embed::Core;
use tokio::sync::broadcast;

use super::types::{ContextItem, RuntimeSnapshot};
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
}

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
        }
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

    fn submit(&self, _input: String) -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async { Err(anyhow::anyhow!(NOT_YET_WIRED)) })
    }

    fn abort(&self) {
        tracing::debug!("[openhuman_runtime] abort ignored: {NOT_YET_WIRED}");
    }

    fn new_session(&self) {
        tracing::debug!("[openhuman_runtime] new_session ignored: {NOT_YET_WIRED}");
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
