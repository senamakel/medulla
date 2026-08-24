//! The feedback board half of [`CloudRuntime`].
//!
//! The board sits on a different route family from the orchestration API the
//! runtime's own client is built for, so each call mints a short-lived
//! [`MedullaClient`] against the configured deployment with the *current* stored
//! token, rather than reusing the one built at construction — a re-login can
//! replace the token while the app is running, and a captured client would keep
//! presenting the retired one.
//!
//! Reads degrade to "no board here" (`Ok(None)`), which the UI renders as a
//! sign-in hint; mutations fail loudly, because silently succeeding at a vote
//! that never reached the backend is worse than saying it did not.

use futures::future::BoxFuture;

use super::CloudRuntime;
use crate::client::{
    FeedbackComment, FeedbackDetail, FeedbackItem, FeedbackPage, FeedbackQuery, FeedbackSubmission,
    FeedbackType, MedullaClient,
};

impl CloudRuntime {
    /// Build a backend client from the configured URL and the stored session.
    ///
    /// `Ok(None)` means this host has no board to talk to — unconfigured, or
    /// signed out. Callers decide whether that is an empty surface or an error.
    async fn feedback_client(base_url: Option<String>) -> anyhow::Result<Option<MedullaClient>> {
        let Some(base_url) = base_url else {
            return Ok(None);
        };
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let jwt = crate::auth::session_token(&env);
        Ok(jwt.map(|jwt| MedullaClient::new(base_url, jwt)))
    }

    /// The client a board *mutation* needs, or the error explaining its absence.
    async fn feedback_client_or_err(base_url: Option<String>) -> anyhow::Result<MedullaClient> {
        Self::feedback_client(base_url).await?.ok_or_else(|| {
            anyhow::anyhow!("the feedback board requires a signed-in backend connection")
        })
    }

    /// The one piece every board call moves into its `'static` future.
    fn feedback_context(&self) -> Option<String> {
        self.backend_base_url.clone()
    }

    /// One page of the board, or `None` when this host has no backend.
    pub(super) fn board_list(
        &self,
        query: FeedbackQuery,
    ) -> BoxFuture<'static, anyhow::Result<Option<FeedbackPage>>> {
        let base_url = self.feedback_context();
        Box::pin(async move {
            let Some(client) = Self::feedback_client(base_url).await? else {
                return Ok(None);
            };
            Ok(Some(client.list_feedback(&query).await?))
        })
    }

    /// One board item with its comments.
    pub(super) fn board_detail(
        &self,
        id: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackDetail>> {
        let base_url = self.feedback_context();
        Box::pin(async move {
            let client = Self::feedback_client_or_err(base_url).await?;
            Ok(client.get_feedback(&id).await?)
        })
    }

    /// Cast, change, or retract a vote.
    pub(super) fn board_vote(
        &self,
        id: String,
        value: i8,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackItem>> {
        let base_url = self.feedback_context();
        Box::pin(async move {
            let client = Self::feedback_client_or_err(base_url).await?;
            Ok(client.vote_feedback(&id, value).await?)
        })
    }

    /// Post a comment on a board item.
    pub(super) fn board_comment(
        &self,
        id: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackComment>> {
        let base_url = self.feedback_context();
        Box::pin(async move {
            let client = Self::feedback_client_or_err(base_url).await?;
            Ok(client.comment_feedback(&id, &body).await?)
        })
    }

    /// Submit new feedback for moderation.
    pub(super) fn board_submit(
        &self,
        kind: FeedbackType,
        title: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackSubmission>> {
        let base_url = self.feedback_context();
        Box::pin(async move {
            let client = Self::feedback_client_or_err(base_url).await?;
            Ok(client.submit_feedback(kind, &title, &body).await?)
        })
    }
}
