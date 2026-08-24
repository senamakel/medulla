//! Building the backend client this runtime drives, and deciding whether a host
//! can actually use it.
//!
//! # Why readiness is three states, not two
//!
//! A host answers each differently: run, sign in, or stop. Collapsing the last
//! two would either send an operator to a login screen that cannot fix a missing
//! base URL, or refuse to start over a state one keystroke resolves.
//!
//! This replaces the probe that used to live in `core_host`, which asked a
//! booted OpenHuman core to list sessions and classified the `data.kind` on the
//! error it returned. Two of those kinds — "no session token", "no base URL" —
//! were the core reporting a decision this host had already made when it
//! configured the core, so the classification round-tripped through an RPC to
//! learn something already in hand. They are checked directly now, and only
//! genuine reachability costs a call.

use std::collections::HashMap;

use crate::client::MedullaClient;
use crate::config::BackendConfig;

/// Whether this host can reach a Medulla backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    /// The orchestration API answered — the backend is usable as a runtime.
    Ready,
    /// Reachable, but nobody is signed in. The host should run its login flow.
    SignedOut,
    /// There is no backend to talk to: no base URL configured. Carries an
    /// operator-safe message (no URLs, no tokens).
    Unusable(String),
}

/// Build the client this runtime drives from a loaded config and the process
/// environment.
///
/// The bearer comes from [`crate::auth::resolve_backend_token`], so an inline
/// `backend.token`, the `backend.tokenEnv` variable and the stored session are
/// consulted in that order — one precedence chain, shared with every other
/// backend-facing surface.
///
/// Returns `None` when the config names no backend or nothing yields a token;
/// use [`readiness`] to tell those two apart.
pub fn client_from_config(
    env: &HashMap<String, String>,
    backend: &BackendConfig,
) -> Option<MedullaClient> {
    let base_url = backend.base_url.trim();
    if base_url.is_empty() {
        return None;
    }
    let session = crate::auth::session_token(env);
    let token = crate::auth::resolve_backend_token(env, backend, session.as_deref())?;
    Some(MedullaClient::new(base_url.to_string(), token))
}

/// Classify what this host can do with the configured backend, without a call.
///
/// Cheap and offline: both non-ready states are local facts. A token that has
/// since expired still reads as [`Readiness::Ready`] here and surfaces as a 401
/// on the call that used it — which is the right place for it, because a flaky
/// network must never read as "sign in again".
pub fn readiness(env: &HashMap<String, String>, backend: &BackendConfig) -> Readiness {
    if backend.base_url.trim().is_empty() {
        return Readiness::Unusable("no backend base URL is configured".to_string());
    }
    let session = crate::auth::session_token(env);
    match crate::auth::resolve_backend_token(env, backend, session.as_deref()) {
        Some(_) => Readiness::Ready,
        None => Readiness::SignedOut,
    }
}

/// Confirm the backend actually answers, for a host that wants to know before
/// it paints.
///
/// Uses the session list because it is the cheapest read that goes through the
/// same client every drive method does — a host that can list sessions can
/// submit to one. A rejected call reads as [`Readiness::SignedOut`] only when
/// the backend says the credential is bad; every other failure is transient and
/// stays [`Readiness::Ready`], so a dropped connection does not send an operator
/// to a login screen.
pub async fn probe(client: &MedullaClient) -> Readiness {
    match client.list_sessions().await {
        Ok(_) => Readiness::Ready,
        Err(err) if is_unauthorized(&err) => Readiness::SignedOut,
        Err(err) => {
            tracing::debug!("[cloud_runtime] readiness probe failed, assuming ready: {err}");
            Readiness::Ready
        }
    }
}

/// Whether a client error means the credential was rejected.
///
/// Matched on the status *and* the backend's `errorCode`, because the two carry
/// different halves of the answer: a 401 is unambiguous, and `TOKEN_EXPIRED`
/// arrives on some routes inside a 200-shaped `{"success": false}` envelope
/// where no status is attached at all.
fn is_unauthorized(err: &crate::client::ClientError) -> bool {
    let crate::client::ClientError::Api {
        status, error_code, ..
    } = err
    else {
        return false;
    };
    matches!(status, Some(401 | 403))
        || matches!(
            error_code.as_deref(),
            Some("TOKEN_EXPIRED" | "SESSION_EXPIRED" | "UNAUTHORIZED")
        )
}

#[cfg(test)]
mod tests;
