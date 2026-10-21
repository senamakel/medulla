//! Compatibility re-exports for the extracted provider registry.

pub use medulla_providers::detect::{
    build_resumed_run_args, build_run_args, detect_providers, extract_session_id, make_path_lookup,
    provider_bin, provider_name, session_id_field, supports_stdin, ExistsOnPath, DAEMON_PROVIDERS,
};
