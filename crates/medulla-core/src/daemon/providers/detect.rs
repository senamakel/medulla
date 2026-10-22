//! Compatibility re-exports for the extracted provider registry.

pub use medulla_providers::provider_bin;
pub use medulla_providers::{
    build_resumed_run_args, build_run_args, detect_providers, extract_session_id, make_path_lookup,
    provider_name, supports_stdin, DAEMON_PROVIDERS,
};
