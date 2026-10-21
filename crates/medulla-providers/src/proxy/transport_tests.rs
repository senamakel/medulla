//! Tests for the proxy transport path mapping and credential minting.

use super::*;

#[test]
fn mounts_map_onto_their_upstream_dialects() {
    assert_eq!(
        UpstreamShape::Anthropic.upstream_base(OPENROUTER_ROOT),
        "https://openrouter.ai/api"
    );
    assert_eq!(
        UpstreamShape::OpenAi.upstream_base(OPENROUTER_ROOT),
        "https://openrouter.ai/api/v1"
    );
    // A trailing slash on the configured root must not double up.
    assert_eq!(
        UpstreamShape::OpenAi.upstream_base("https://openrouter.ai/api/"),
        "https://openrouter.ai/api/v1"
    );
}

#[test]
fn paths_split_into_a_mount_and_a_remainder() {
    use super::serve::split_mount;

    assert_eq!(
        split_mount("/anthropic/v1/messages"),
        Some((UpstreamShape::Anthropic, "/v1/messages"))
    );
    assert_eq!(
        split_mount("/openai/chat/completions"),
        Some((UpstreamShape::OpenAi, "/chat/completions"))
    );
    assert_eq!(
        split_mount("/anthropic"),
        Some((UpstreamShape::Anthropic, ""))
    );
    assert_eq!(split_mount("/unknown/v1"), None);
    assert_eq!(split_mount("/"), None);
}

#[test]
fn upstream_urls_carry_the_query_string() {
    use super::serve::upstream_url;

    assert_eq!(
        upstream_url(
            OPENROUTER_ROOT,
            UpstreamShape::Anthropic,
            "/v1/messages",
            None
        ),
        "https://openrouter.ai/api/v1/messages"
    );
    assert_eq!(
        upstream_url(
            OPENROUTER_ROOT,
            UpstreamShape::OpenAi,
            "/models",
            Some("limit=5")
        ),
        "https://openrouter.ai/api/v1/models?limit=5"
    );
}

#[test]
fn tokens_are_minted_once_per_credential() {
    let handle = ProxyHandle::start(OPENROUTER_ROOT.to_string()).expect("proxy starts");

    let first = handle.endpoint_for_key("sk-or-one");
    let again = handle.endpoint_for_key("sk-or-one");
    let other = handle.endpoint_for_key("sk-or-two");

    assert_eq!(first.token, again.token, "same key reuses its token");
    assert_ne!(
        first.token, other.token,
        "distinct keys get distinct tokens"
    );
    assert!(first.token.starts_with("mdl-"));
    assert_eq!(first.port, handle.port());
    assert_ne!(handle.port(), 0);
}
