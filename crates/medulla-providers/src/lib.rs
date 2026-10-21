//! Pure provider metadata and configuration shared by Medulla's SDK and tools.
//!
//! This crate owns provider detection, argv/environment shaping, routing
//! decisions, harness preferences, custom harness presets, and budget/router
//! configuration types. It performs no provider execution or capability probe.

mod config;
mod custom_harnesses;
mod detect;
mod env;
mod harness_choice;
mod routing;

pub use config::{
    BudgetConfig, ProviderBudgetConfig, RouterConfig, RouterProviderConfig,
};
pub use custom_harnesses::{
    load_custom_harnesses, load_layered_custom_harnesses, CustomHarnessConfig,
    CODEX_CONTEXT_WINDOW_ENV, CODEX_DISPLAY_NAME_ENV, CODEX_EFFORT_ENV, CODEX_OVERRIDES_ENV,
    EDITOR_LINE_FORMAT, OPENROUTER_ANTHROPIC_URL, OPENROUTER_API_KEY_ENV,
    OPENROUTER_OPENAI_URL,
};
pub use detect::{
    build_resumed_run_args, build_run_args, detect_providers, extract_session_id, make_path_lookup,
    provider_name, provider_bin as detected_provider_bin, session_id_field, supports_stdin,
    ExistsOnPath, DAEMON_PROVIDERS,
};
pub use env::{
    bin_is_overridden, model_override, provider_args, provider_bin, router_env,
    RouterInjection,
};
pub use harness_choice::{HarnessChoice, HarnessPreference, HarnessSelector};
pub use routing::{
    can_resume,
};
