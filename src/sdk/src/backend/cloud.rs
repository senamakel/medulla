//! [`Backend`] over the TinyHumans backend, through [`MedullaClient`].
//!
//! Each call mints a short-lived [`MedullaClient`] against the configured
//! deployment with the *current* credential rather than holding one built at
//! construction — a re-login can replace the token while the app is running,
//! and a captured client would keep presenting the retired one.
//!
//! The bearer is resolved through [`crate::auth::resolve_backend_token`], the
//! same chain sign-in uses. Reading the stored session directly is what this
//! used to do, and it left every operator authenticated through
//! `backend.token` or `backend.tokenEnv` with a board that reported itself
//! unavailable.
//!
//! Reads degrade to "nothing here" (`Ok(None)`), which the UI renders as a
//! sign-in hint; mutations fail loudly, because silently succeeding at a vote
//! that never reached the backend is worse than saying it did not.

use futures::future::BoxFuture;

use super::Backend;
use crate::client::{
    FeedbackComment, FeedbackDetail, FeedbackItem, FeedbackPage, FeedbackQuery, FeedbackSubmission,
    FeedbackType, MedullaClient,
};
use crate::config::BackendConfig;

/// The product backend: the configured deployment plus whoever is signed in.
#[derive(Debug, Clone)]
pub struct CloudBackend {
    backend: BackendConfig,
}

impl CloudBackend {
    /// A backend over the deployment `backend` names.
    pub fn new(backend: BackendConfig) -> Self {
        Self { backend }
    }

    /// Build a client from the configured URL and the current credential.
    ///
    /// `None` means this host is signed out. Callers decide whether that is an
    /// empty surface or an error.
    fn client(backend: &BackendConfig) -> Option<MedullaClient> {
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let session = crate::auth::session_token(&env);
        let jwt = crate::auth::resolve_backend_token(&env, backend, session.as_deref());
        jwt.map(|jwt| MedullaClient::new(backend.base_url.clone(), jwt))
    }

    /// The client a *mutation* needs, or the error explaining its absence.
    fn client_or_err(backend: &BackendConfig) -> anyhow::Result<MedullaClient> {
        Self::client(backend).ok_or_else(|| {
            anyhow::anyhow!("the feedback board requires a signed-in backend connection")
        })
    }
}

impl Backend for CloudBackend {
    fn describe(&self) -> String {
        format!("backend {}", crate::config::display_host(&self.backend.base_url))
    }

    fn team_usage(&self) -> BoxFuture<'static, anyhow::Result<Option<serde_json::Value>>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let Some(client) = Self::client(&backend) else {
                return Ok(None);
            };
            Ok(Some(client.team_usage().await?))
        })
    }

    fn logout(&self) -> BoxFuture<'static, anyhow::Result<()>> {
        let backend = self.backend.clone();
        // Removing the store's file is the whole logout as far as *this* host's
        // own credential goes — there is no keychain entry or profile beside it
        // that a next start could read back.
        //
        // It is not the whole story for the process, though: `backend.token` and
        // `backend.tokenEnv` outrank the store in
        // [`crate::auth::resolve_backend_token`], so an operator running on an
        // inline token or a custom variable stays authenticated after a
        // "successful" logout. That is worth an error rather than a silent
        // half-measure — telling someone they signed out while they are still
        // signed in is the failure mode logging out exists to prevent.
        Box::pin(async move {
            let env: std::collections::HashMap<String, String> = std::env::vars().collect();
            crate::auth::clear(&env)
                .map_err(|e| anyhow::anyhow!("the stored session could not be removed: {e}"))?;
            // Resolved against the *real* config, with `session` deliberately
            // `None`: the store has just been cleared, so anything that still
            // resolves is by definition a source that outranks it.
            if crate::auth::resolve_backend_token(&env, &backend, None).is_some() {
                let source = if backend.token.is_some() {
                    "`backend.token` in the config".to_string()
                } else {
                    format!("the `{}` environment variable", backend.token_env)
                };
                anyhow::bail!(
                    "the stored session was cleared, but this process is still \
                     authenticated from {source} — remove it to finish signing out"
                );
            }
            Ok(())
        })
    }

    fn list_feedback(
        &self,
        query: FeedbackQuery,
    ) -> BoxFuture<'static, anyhow::Result<Option<FeedbackPage>>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let Some(client) = Self::client(&backend) else {
                return Ok(None);
            };
            Ok(Some(client.list_feedback(&query).await?))
        })
    }

    fn feedback_detail(&self, id: String) -> BoxFuture<'static, anyhow::Result<FeedbackDetail>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let client = Self::client_or_err(&backend)?;
            Ok(client.get_feedback(&id).await?)
        })
    }

    fn vote_feedback(
        &self,
        id: String,
        value: i8,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackItem>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let client = Self::client_or_err(&backend)?;
            Ok(client.vote_feedback(&id, value).await?)
        })
    }

    fn comment_feedback(
        &self,
        id: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackComment>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let client = Self::client_or_err(&backend)?;
            Ok(client.comment_feedback(&id, &body).await?)
        })
    }

    fn submit_feedback(
        &self,
        kind: FeedbackType,
        title: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackSubmission>> {
        let backend = self.backend.clone();
        Box::pin(async move {
            let client = Self::client_or_err(&backend)?;
            Ok(client.submit_feedback(kind, &title, &body).await?)
        })
    }
}
