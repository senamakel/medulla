use super::*;

#[test]
fn acp_env_nests_the_dotted_override_keys_into_json() {
    let dir = tempfile::tempdir().unwrap();
    write_codex_cache(dir.path());
    let env = routed_env(dir.path());

    let rendered: HashMap<String, String> =
        acp_env(HarnessProvider::Codex, Some("vendor/model-1"), &env)
            .unwrap()
            .into_iter()
            .collect();

    assert_eq!(
        rendered.get(MODEL_PROVIDER_ENV).map(String::as_str),
        Some(PROVIDER_ID)
    );
    let config: Value = serde_json::from_str(rendered.get(CONFIG_ENV).unwrap()).unwrap();
    assert_eq!(config["model_provider"], PROVIDER_ID);
    assert_eq!(
        config["model_providers"][PROVIDER_ID]["base_url"],
        "http://127.0.0.1:7777/openai"
    );
    assert_eq!(
        config["model_providers"][PROVIDER_ID]["env_key"],
        "OPENAI_API_KEY"
    );
    assert_eq!(
        config["model_providers"][PROVIDER_ID]["wire_api"],
        "responses"
    );
    assert_eq!(config["preferred_auth_method"], "apikey");
    assert!(config["model_catalog_json"].is_string());
}

/// ACP has no `-m`, so the model has to be part of the config document or the
/// session opens on Codex's own default — a model the routed catalog does not
/// describe and the preset never asked for.
#[test]
fn acp_env_selects_the_routed_model_because_argv_cannot() {
    let dir = tempfile::tempdir().unwrap();
    write_codex_cache(dir.path());
    let env = routed_env(dir.path());

    let rendered: HashMap<String, String> =
        acp_env(HarnessProvider::Codex, Some("vendor/model-1"), &env)
            .unwrap()
            .into_iter()
            .collect();
    let config: Value = serde_json::from_str(rendered.get(CONFIG_ENV).unwrap()).unwrap();

    assert_eq!(config["model"], "vendor/model-1");
}

/// The two transports must not disagree about where a routed run points, so
/// both renderings come from the same override list.
#[test]
fn acp_env_and_launch_args_carry_the_same_overrides() {
    let dir = tempfile::tempdir().unwrap();
    write_codex_cache(dir.path());
    let env = routed_env(dir.path());

    let args = launch_args(HarnessProvider::Codex, Some("vendor/model-1"), &env).unwrap();
    // `overrides` here is this file's argv parser, not the shared override list.
    let from_argv = overrides(&args);
    let rendered: HashMap<String, String> =
        acp_env(HarnessProvider::Codex, Some("vendor/model-1"), &env)
            .unwrap()
            .into_iter()
            .collect();
    let config: Value = serde_json::from_str(rendered.get(CONFIG_ENV).unwrap()).unwrap();

    assert!(!from_argv.is_empty(), "the CLI seam must still carry them");
    for (key, value) in from_argv {
        let mut cursor = &config;
        for segment in key.split('.') {
            cursor = &cursor[segment];
        }
        assert_eq!(
            cursor,
            &Value::String(value),
            "ACP must carry {key} exactly as the CLI seam does"
        );
    }
}

/// An unrouted run gets nothing at all: no endpoint means no provider block, and
/// setting one would move a run that never asked to be routed off the
/// operator's own account.
#[test]
fn acp_env_is_empty_without_a_routed_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    write_codex_cache(dir.path());
    let mut env = routed_env(dir.path());
    env.remove("OPENAI_BASE_URL");

    assert!(
        acp_env(HarnessProvider::Codex, Some("vendor/model-1"), &env)
            .unwrap()
            .is_empty()
    );
}

/// Neither does a preset that did not opt in, nor another harness entirely.
#[test]
fn acp_env_is_empty_without_the_opt_in_or_for_another_harness() {
    let dir = tempfile::tempdir().unwrap();
    write_codex_cache(dir.path());
    let mut opted_out = routed_env(dir.path());
    opted_out.remove(OVERRIDES_ENV);

    assert!(
        acp_env(HarnessProvider::Codex, Some("vendor/model-1"), &opted_out)
            .unwrap()
            .is_empty()
    );
    assert!(acp_env(
        HarnessProvider::Claude,
        Some("vendor/model-1"),
        &routed_env(dir.path())
    )
    .unwrap()
    .is_empty());
}
