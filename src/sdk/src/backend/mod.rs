//! What the TUI still asks the TinyHumans backend for, behind one trait.
//!
//! The backend's part in Medulla is small and account-shaped: who is signed
//! in and what their plan entitles them to (read through
//! [`crate::client::MedullaClient::me`] by [`crate::access`]), the account's
//! usage for Settings › Usage, signing out, and the public feedback board.
//! Nothing here touches sessions, hosts, or dispatch — those are local.
//!
//! [`Backend`] is the trait every surface is written against. [`CloudBackend`]
//! is the product implementation over [`MedullaClient`]; [`MockBackend`] is
//! the offline stand-in behind `--mock` and the TUI's feature suites, with a
//! scripted board that retallies votes and keeps comments for the life of the
//! process.

use futures::future::BoxFuture;

use crate::client::{
    FeedbackComment, FeedbackDetail, FeedbackItem, FeedbackPage, FeedbackQuery, FeedbackSubmission,
    FeedbackType,
};

mod cloud;
pub mod mock;

pub use cloud::CloudBackend;
pub use mock::MockBackend;

/// The backend surface the TUI drives.
///
/// Every method returns a `'static` future so the event loop can spawn it
/// without borrowing the app; a backend that has nothing to answer with
/// reports so explicitly rather than pretending.
pub trait Backend: Send + Sync {
    /// A short human-readable description for the header and logs.
    fn describe(&self) -> String;

    /// Account-level usage for Settings › Usage (`/teams/me/usage`).
    ///
    /// `Ok(None)` means this backend serves no usage document — the mock, or
    /// a signed-out host — which the page renders as a hint rather than an
    /// error.
    fn team_usage(&self) -> BoxFuture<'static, anyhow::Result<Option<serde_json::Value>>> {
        Box::pin(std::future::ready(Ok(None)))
    }

    /// Clear the stored session so the next start opens the login screen.
    fn logout(&self) -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }

    // --- feedback board ----------------------------------------------------
    // `list_feedback` returning `Ok(None)` means "this backend has no board",
    // which the UI renders as a sign-in hint rather than an empty list; the
    // mutating calls fail loudly for that case.

    /// A page of the public feedback board. `Ok(None)` = no board here.
    fn list_feedback(
        &self,
        _query: FeedbackQuery,
    ) -> BoxFuture<'static, anyhow::Result<Option<FeedbackPage>>> {
        Box::pin(std::future::ready(Ok(None)))
    }

    /// One board item with its comments.
    fn feedback_detail(&self, _id: String) -> BoxFuture<'static, anyhow::Result<FeedbackDetail>> {
        Box::pin(std::future::ready(Err(no_feedback_backend())))
    }

    /// Cast, change, or retract a vote (`1`, `-1`, `0`). Returns the item with
    /// recomputed tallies.
    fn vote_feedback(
        &self,
        _id: String,
        _value: i8,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackItem>> {
        Box::pin(std::future::ready(Err(no_feedback_backend())))
    }

    /// Post a comment on a board item.
    fn comment_feedback(
        &self,
        _id: String,
        _body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackComment>> {
        Box::pin(std::future::ready(Err(no_feedback_backend())))
    }

    /// Submit new feedback. A moderation rejection is a successful call with
    /// [`FeedbackSubmission::accepted`] false — not an error.
    fn submit_feedback(
        &self,
        _kind: FeedbackType,
        _title: String,
        _body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackSubmission>> {
        Box::pin(std::future::ready(Err(no_feedback_backend())))
    }
}

/// The error every feedback mutation returns on a backend with no board.
fn no_feedback_backend() -> anyhow::Error {
    anyhow::anyhow!("the feedback board requires a signed-in backend connection")
}

/// A backend with no account and no board: what a signed-out host drives.
///
/// Every read reports "nothing here" and every mutation refuses, so the
/// surfaces render their sign-in hints instead of waiting on a request that
/// was never sent.
#[derive(Debug, Default, Clone, Copy)]
pub struct OfflineBackend;

impl Backend for OfflineBackend {
    fn describe(&self) -> String {
        "offline".to_string()
    }
}
