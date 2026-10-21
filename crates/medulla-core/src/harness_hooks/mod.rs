//! Core adapters around the provider-independent harness customization crate.

pub use medulla_harness::harness_hooks::*;
pub mod acp;
mod grant;

pub use grant::{seed_hook_grant, HookGrantGuard};

/// Resolve configuration into the policy shared by every harness spawn door.
#[must_use]
pub fn launch_policy(config: &crate::config::TuiConfig) -> LaunchPolicy {
    let hooks = if config.hook_defaults.enabled {
        config
            .hooks
            .with_builtin(builtin::hooks(&builtin::medulla_bin()))
    } else {
        config.hooks.clone()
    };
    LaunchPolicy {
        attribution: config.attribution.commit,
        hooks,
    }
}
