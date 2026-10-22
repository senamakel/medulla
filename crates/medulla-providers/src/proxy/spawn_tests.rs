//! Tests for router and environment mutation at the spawn seam.

use std::collections::HashMap;

use crate::RouterConfig;
use medulla_types::HarnessProvider;

use super::*;

fn openrouter_router() -> RouterConfig {
    RouterConfig {
        base_url: Some(OPENROUTER_ROOT.to_string()),
        api_key_env: Some("OPENROUTER_API_KEY".to_string()),
        ..RouterConfig::default()
    }
}

#[test]
fn route_spawn_scrubs_the_real_key_and_rewrites_the_router() {
    let mut router = Some(openrouter_router());
    let mut env: HashMap<String, String> = HashMap::from([
        ("OPENROUTER_API_KEY".to_string(), "sk-or-real".to_string()),
        ("PATH".to_string(), "/usr/bin".to_string()),
        // Keeps the listener off the network: the run never reaches upstream in
        // this test, but starting one that could would be wrong regardless.
        (
            UPSTREAM_URL_ENV.to_string(),
            "http://127.0.0.1:1/api".to_string(),
        ),
    ]);
    route_spawn(HarnessProvider::Claude, &mut router, &mut env).expect("routed");

    // The real key is gone — a harness in this environment has nothing to reach
    // OpenRouter with except the loopback token.
    assert!(!env.contains_key("OPENROUTER_API_KEY"));
    let token = env.get(PROXY_TOKEN_ENV).expect("token exported");
    assert!(token.starts_with("mdl-"));
    assert_eq!(token.len(), "mdl-".len() + 32);
    assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
    assert_eq!(
        env.get(UPSTREAM_URL_ENV).map(String::as_str),
        Some("http://127.0.0.1:1/api")
    );
    assert_eq!(
        env.keys()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>(),
        ["PATH", UPSTREAM_URL_ENV, PROXY_TOKEN_ENV]
            .into_iter()
            .collect()
    );

    let router = router.expect("router kept");
    assert_eq!(router.api_key_env.as_deref(), Some(PROXY_TOKEN_ENV));
    assert!(router
        .base_url_for("claude")
        .expect("claude routed")
        .starts_with("http://127.0.0.1:"));
}
