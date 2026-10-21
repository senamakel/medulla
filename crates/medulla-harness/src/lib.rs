//! Provider-independent customization for Medulla-launched coding harnesses.

pub mod attribution;
pub mod codex_overrides;
mod grants;
pub mod harness_hooks;

pub use grants::{HookGrant, HookGrantMinter, HOOK_GRANT_ENV, HOOK_SOCKET_ENV};
