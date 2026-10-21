//! Core implementation of the harness crate's hook grant seam.

use std::collections::HashMap;

use medulla_harness::{HookGrant, HookGrantMinter};

struct CoreHookGrantMinter;

impl HookGrantMinter for CoreHookGrantMinter {
    fn mint(&self, session: &str) -> Option<HookGrant> {
        #[cfg(feature = "workflows")]
        {
            crate::mcp::local_hook_grant(session).map(|(socket, token)| HookGrant { socket, token })
        }
        #[cfg(not(feature = "workflows"))]
        {
            let _ = session;
            None
        }
    }

    fn revoke(&self, session: &str) {
        #[cfg(feature = "workflows")]
        crate::mcp::revoke_session(session);
        #[cfg(not(feature = "workflows"))]
        let _ = session;
    }
}

/// Seed a spawned harness with a hook-only grant when this process serves a control plane.
pub fn seed_hook_grant(session: impl Into<String>, env: &mut HashMap<String, String>) -> HookGrantGuard {
    let session = session.into();
    match CoreHookGrantMinter.mint(&session) {
        Some(grant) => {
            env.insert(
                medulla_harness::HOOK_SOCKET_ENV.to_string(),
                grant.socket.to_string_lossy().into_owned(),
            );
            env.insert(medulla_harness::HOOK_GRANT_ENV.to_string(), grant.token);
            HookGrantGuard(Some(session))
        }
        None => HookGrantGuard(None),
    }
}

/// Revokes a minted grant when the owning spawn ends.
pub struct HookGrantGuard(Option<String>);

impl Drop for HookGrantGuard {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            CoreHookGrantMinter.revoke(&session);
        }
    }
}

#[cfg(test)]
#[path = "grant_tests.rs"]
mod tests;
