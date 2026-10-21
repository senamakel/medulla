//! Core implementation of the harness crate's hook grant seam.

use std::collections::HashMap;
use std::sync::Arc;

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
pub fn seed_hook_grant(
    session: impl Into<String>,
    env: &mut HashMap<String, String>,
) -> medulla_harness::HookGrantGuard {
    medulla_harness::seed_hook_grant(session, env, Arc::new(CoreHookGrantMinter))
}

pub use medulla_harness::HookGrantGuard;

#[cfg(test)]
#[path = "grant_tests.rs"]
mod tests;
