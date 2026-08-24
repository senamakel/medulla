//! The Medulla-owned app session store: where a verified JWT lives between runs.
//!
//! # Why this is here again
//!
//! Sessions used to live in the embedded OpenHuman core, reached through
//! `core_host::auth` over four `openhuman.auth_*` RPCs. That was the right
//! answer while the core *was* the runtime: two stores meant two answers to "am
//! I signed in?", and `medulla login` could succeed while the core — whose
//! session actually drove the runtime — stayed signed out.
//!
//! With the core gone there is no second store to disagree with, so the reason
//! for deferring outward is gone with it. This module is the single place a
//! session lives, which is the same invariant, now satisfied by owning the store
//! rather than by delegating it.
//!
//! # The store is validated on write, not on read
//!
//! [`store`] calls `GET /auth/me` with the candidate token and refuses to
//! persist one the backend rejects — the same contract
//! `openhuman.auth_store_session` had, and the reason a bad paste fails at
//! `medulla login` instead of on every later call. Reads are then cheap and
//! offline: a token that has since expired surfaces as a 401 from the call that
//! used it, which is where the caller can act on it.
//!
//! # Where the file goes
//!
//! `<root>/<account id>/session.json`, the account-scoped home from
//! [`crate::home::medulla_home`]. [`store`] resolves the account id from the
//! `/auth/me` response it just verified and publishes it as the active account
//! before writing, so the file lands under the account it belongs to rather than
//! under whichever one happened to be active. On unix the file is created 0600.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::LoginError;

/// The session file's name inside an account home.
const SESSION_FILE: &str = "session.json";

/// Who this install is signed in as.
///
/// Field-for-field the projection `core_host::auth::AuthState` presented, so the
/// TUI's account surface is unchanged by the move.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthState {
    /// Whether a usable session is stored.
    pub is_authenticated: bool,
    /// The signed-in account's id, when one is known.
    pub user_id: Option<String>,
}

/// The persisted session record.
///
/// `base_url` is recorded for diagnostics — which deployment issued this token —
/// and deliberately **not** matched against the configured backend on read. That
/// comparison used to exist and rejected perfectly good tokens whenever the two
/// spellings drifted (a trailing slash was enough), which is a worse failure
/// than the mismatch it guarded against.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSession {
    /// The verified bearer token.
    pub token: String,
    /// The account id from the `/auth/me` response that verified the token.
    #[serde(default)]
    pub user_id: Option<String>,
    /// The backend origin that issued the token.
    #[serde(default)]
    pub base_url: String,
}

/// The session file for an account home.
fn location(home: &Path) -> PathBuf {
    home.join(SESSION_FILE)
}

/// Verify `jwt` against the backend and persist it as this install's session.
///
/// The token is checked with `GET /auth/me` before anything is written, so a
/// token for a different deployment — or an expired one — fails here rather than
/// being stored and failing on every later call. The account id in that response
/// becomes the active account, so the session file lands in that account's home.
///
/// # Errors
///
/// [`LoginError::Backend`] when `/auth/me` rejects the token or cannot be
/// reached, and [`LoginError::Io`] when the account home or the file itself
/// cannot be written.
pub async fn store(
    env: &HashMap<String, String>,
    base_url: &str,
    jwt: &str,
) -> Result<AuthState, LoginError> {
    let me = crate::client::MedullaClient::new(base_url, jwt)
        .me()
        .await
        .map_err(|e| {
            LoginError::Backend(format!("session validation failed (GET /auth/me): {e}"))
        })?;

    let user_id = crate::auth::user_id_from_me(&me);
    let root = crate::home::medulla_root(env);

    // Publish the account *before* resolving its home: `medulla_home` reads the
    // marker this writes, so the order is what puts the file under the account
    // the token actually belongs to instead of the previously active one.
    if let Some(id) = user_id.as_deref() {
        crate::home::user::write_active_user_id(&root, id).map_err(LoginError::Io)?;
    }
    let home = crate::home::medulla_home(env);
    std::fs::create_dir_all(&home).map_err(LoginError::Io)?;

    let record = StoredSession {
        token: jwt.to_string(),
        user_id: user_id.clone(),
        base_url: base_url.trim_end_matches('/').to_string(),
    };
    let body = serde_json::to_vec_pretty(&record)
        .map_err(|e| LoginError::Backend(format!("could not encode session: {e}")))?;
    write_private(&location(&home), &body).map_err(LoginError::Io)?;

    tracing::debug!("[auth] app session stored");
    Ok(AuthState {
        is_authenticated: true,
        user_id,
    })
}

/// The stored session token, if there is one.
///
/// `Ok(None)` is the signed-out state, not a failure. Every reader that needs
/// the JWT itself takes it from here rather than reading a file of its own, so
/// there is exactly one place a session lives.
pub fn token(env: &HashMap<String, String>) -> Option<String> {
    read(env).map(|s| s.token).filter(|t| !t.trim().is_empty())
}

/// The full stored session record, if one is readable.
///
/// A malformed or unreadable file reads as signed out rather than as an error:
/// the only useful recovery is to sign in again, and failing a command over a
/// corrupt cache would block exactly that.
pub fn read(env: &HashMap<String, String>) -> Option<StoredSession> {
    let path = location(&crate::home::medulla_home(env));
    let body = std::fs::read(&path).ok()?;
    match serde_json::from_slice::<StoredSession>(&body) {
        Ok(session) => Some(session),
        Err(err) => {
            tracing::debug!("[auth] ignoring unreadable session at {path:?}: {err}");
            None
        }
    }
}

/// Who this install is signed in as, for the Account surface.
pub fn state(env: &HashMap<String, String>) -> AuthState {
    match read(env) {
        Some(s) if !s.token.trim().is_empty() => AuthState {
            is_authenticated: true,
            user_id: s.user_id,
        },
        _ => AuthState::default(),
    }
}

/// Forget the stored session.
///
/// Idempotent: clearing when already signed out succeeds. Returns the path when
/// a file was actually removed, so `logout` can say what it did.
pub fn clear(env: &HashMap<String, String>) -> Option<PathBuf> {
    let path = location(&crate::home::medulla_home(env));
    if path.exists() && std::fs::remove_file(&path).is_ok() {
        tracing::debug!("[auth] app session cleared");
        return Some(path);
    }
    None
}

/// Write `body` to `path` with owner-only permissions.
///
/// The mode is set on the handle before the bytes are written, not with a
/// `set_permissions` call afterwards: the gap between a world-readable create
/// and a later chmod is exactly long enough for another process on the machine
/// to read the token out.
pub(super) fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(body)?;
    file.sync_all()
}
