//! Compatibility re-exports for the extracted provider registry.

pub use medulla_providers::{
    build_resumed_run_args, build_run_args, detect_providers, extract_session_id, make_path_lookup,
    provider_name, session_id_field, supports_stdin, ExistsOnPath, DAEMON_PROVIDERS,
};
pub use medulla_providers::detected_provider_bin as provider_bin;
