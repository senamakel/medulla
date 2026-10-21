//! Narrow seam between harness customization and a host's control plane.

use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;

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

/// Seed a harness environment and revoke the minted grant when its guard drops.
pub fn seed_hook_grant(
    session: impl Into<String>,
    env: &mut HashMap<String, String>,
    minter: Arc<dyn HookGrantMinter>,
) -> HookGrantGuard {
    let session = session.into();
    match minter.mint(&session) {
        Some(grant) => {
            env.insert(HOOK_SOCKET_ENV.to_owned(), grant.socket.to_string_lossy().into_owned());
            env.insert(HOOK_GRANT_ENV.to_owned(), grant.token);
            HookGrantGuard(Some((session, minter)))
        }
        None => HookGrantGuard(None),
    }
}

/// RAII guard that revokes a hook grant when its spawn ends.
pub struct HookGrantGuard(Option<(String, Arc<dyn HookGrantMinter>)>);

impl Drop for HookGrantGuard {
    fn drop(&mut self) {
        if let Some((session, minter)) = self.0.take() {
            minter.revoke(&session);
        }
    }
}

/// Environment variable carrying the local hook-report socket path.
pub const HOOK_SOCKET_ENV: &str = "MEDULLA_HOOK_SOCKET";

/// Environment variable carrying the opaque hook-report credential.
pub const HOOK_GRANT_ENV: &str = "MEDULLA_HOOK_GRANT";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeMinter(Mutex<Vec<String>>);

    impl HookGrantMinter for FakeMinter {
        fn mint(&self, _session: &str) -> Option<HookGrant> {
            Some(HookGrant { socket: "/tmp/hook.sock".into(), token: "opaque".into() })
        }

        fn revoke(&self, session: &str) {
            self.0.lock().unwrap().push(session.to_owned());
        }
    }

    #[test]
    fn minted_grant_seeds_environment_and_is_revoked_with_guard() {
        let minter = Arc::new(FakeMinter::default());
        let mut env = HashMap::new();
        let guard = seed_hook_grant("session-a", &mut env, minter.clone());
        assert_eq!(env[HOOK_SOCKET_ENV], "/tmp/hook.sock");
        assert_eq!(env[HOOK_GRANT_ENV], "opaque");
        drop(guard);
        assert_eq!(*minter.0.lock().unwrap(), ["session-a"]);
    }

    #[test]
    fn missing_grant_is_a_noop_and_does_not_revoke() {
        struct EmptyMinter;
        impl HookGrantMinter for EmptyMinter {
            fn mint(&self, _session: &str) -> Option<HookGrant> { None }
            fn revoke(&self, _session: &str) { panic!("cannot revoke an unminted grant") }
        }
        let mut env = HashMap::new();
        let _guard = seed_hook_grant("session-b", &mut env, Arc::new(EmptyMinter));
        assert!(env.is_empty());
    }
}
