//! Narrow seam between harness customization and a host's control plane.

use std::path::PathBuf;

/// A hook-only credential handed to one spawned harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookGrant {
    /// Local control socket used by hook reports.
    pub socket: PathBuf,
    /// Opaque credential accepted by the control plane.
    pub token: String,
}

/// Mints and revokes the short-lived credential used by built-in hook commands.
pub trait HookGrantMinter: Send + Sync {
    /// Mint a grant for `session`, or return `None` when no local control plane exists.
    fn mint(&self, session: &str) -> Option<HookGrant>;

    /// Revoke the grant previously minted for `session`.
    fn revoke(&self, session: &str);
}

/// Environment variable carrying the local hook-report socket path.
pub const HOOK_SOCKET_ENV: &str = "MEDULLA_HOOK_SOCKET";

/// Environment variable carrying the opaque hook-report credential.
pub const HOOK_GRANT_ENV: &str = "MEDULLA_HOOK_GRANT";
