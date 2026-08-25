//! Adopt the credential files written before the session store moved.
//!
//! Two stores predate [`super::session`]. A `credentials.json` under the Medulla
//! home (and an older one under the OS config directory) held a real JWT back
//! when Medulla owned its own credential file; it was retired when sessions
//! moved into the embedded OpenHuman core, and from then on `login` and `logout`
//! simply deleted it — a bearer token sitting in a file that no code path can
//! invalidate is strictly worse than one in use.
//!
//! Now that the store is Medulla's again, deleting is the wrong move: the token
//! in that file is the same app session the new store wants, so an install that
//! predates the core is signed in and should stay signed in. The sweep therefore
//! *adopts* — hand the JWT back to the caller, which verifies it through
//! [`super::session::store`] like any other, and remove the old file only once
//! the new one is written.

use std::path::{Path, PathBuf};

use super::types::Credentials;

/// The retired store under the Medulla home.
fn home_location(home: &Path) -> PathBuf {
    home.join("credentials.json")
}

/// The older retired store under the OS config directory.
fn config_dir_location() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("medulla").join("credentials.json"))
}

/// A legacy credential that is worth trying, with the file it came from.
#[derive(Debug, Clone)]
pub struct LegacyCredential {
    /// The file the credential was read from, so the caller can remove it once
    /// the token has been verified and re-stored.
    pub path: PathBuf,
    /// The backend origin recorded alongside the JWT.
    pub base_url: String,
    /// The bearer token itself.
    pub jwt: String,
}

/// Read any legacy credential file, newest location first.
///
/// Best-effort: an unreadable or malformed file is skipped rather than reported,
/// because the only recovery is to sign in again and failing the command that
/// the sweep rides along with would block exactly that. Returns every candidate
/// so a caller that finds the first token rejected can try the next.
pub fn adopt_legacy_credentials(home: &Path) -> Vec<LegacyCredential> {
    let mut found = Vec::new();
    for path in [Some(home_location(home)), config_dir_location()]
        .into_iter()
        .flatten()
    {
        let Ok(body) = std::fs::read(&path) else {
            continue;
        };
        let Ok(creds) = serde_json::from_slice::<Credentials>(&body) else {
            tracing::debug!("[auth] ignoring unreadable legacy credential at {path:?}");
            continue;
        };
        if creds.jwt.trim().is_empty() {
            continue;
        }
        found.push(LegacyCredential {
            path,
            base_url: creds.base_url,
            jwt: creds.jwt,
        });
    }
    found
}

/// Remove a legacy credential file once its token has been re-stored.
///
/// Idempotent and best-effort, for the same reason the read is: a file that
/// cannot be removed (permissions, a read-only mount) is reported by returning
/// `false` rather than by failing the caller's command.
pub fn discard_legacy_credential(path: &Path) -> bool {
    path.exists() && std::fs::remove_file(path).is_ok()
}
