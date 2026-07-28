//! Sign a booted core in, so its Medulla surface has a session to use.
//!
//! # Why this goes through `Core::raw`
//!
//! The typed embed facade models the Medulla surface and configuration; it does
//! not yet model auth. The controllers are registered — `DomainSet::embedded`
//! turns the `platform` family on — so the methods are live, they simply have
//! no typed wrapper upstream yet. `Core::raw` is the facade's documented escape
//! hatch for exactly that, and keeping every use of it in this one module means
//! the JSON and the stringly-typed method names are paid once: when a typed
//! `Auth` facade lands upstream, only this file changes.
//!
//! # Which token this is
//!
//! The core's *app session* JWT, not a Medulla-specific credential. The Medulla
//! orchestration API and the OpenHuman backend are one deployment, so the JWT
//! the login flow verifies against `/auth/me` is the same token
//! `medulla::resolve` looks for. Storing it here is what turns a signed-out core
//! into a usable runtime.

use openhuman_core::embed::{Core, CoreError};

/// RPC that validates a session JWT and writes it to the credential store.
///
/// Canonical `openhuman.<namespace>_<function>` spelling, as the dispatcher
/// expects it.
const STORE_SESSION: &str = "openhuman.auth_store_session";

/// Store `jwt` as the core's app session.
///
/// The core validates the token against the backend before persisting it, so a
/// rejected or expired JWT fails here rather than being written and failing on
/// every later call.
///
/// # Errors
///
/// [`CoreError::Rpc`] carrying the core's message when validation or the write
/// fails — including the case where the operator pasted a token for a different
/// deployment than the one the core resolves.
pub async fn store_session(core: &Core, jwt: &str) -> Result<(), CoreError> {
    let params = serde_json::json!({ "token": jwt });
    core.raw()
        .invoke(STORE_SESSION, params)
        .await
        .map_err(|message| CoreError::Rpc {
            method: STORE_SESSION,
            message,
        })?;
    tracing::debug!("[core_host] app session stored");
    Ok(())
}
