//! End-to-end tests of the HTTP endpoint surface driven against a local TCP
//! stub, asserting on request lines/bodies and the decoded responses as well
//! as transport/decode error paths.

use super::{http_json, spawn_stub};
use crate::client::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Integration: create session / error envelope round-trips
// ---------------------------------------------------------------------------

#[tokio::test]
async fn integration_error_envelope_maps() {
    let body = r#"{"success":false,"error":"nope","errorCode":"TOKEN_EXPIRED"}"#;
    let base = spawn_stub(http_json("HTTP/1.1 401 Unauthorized", body)).await;
    let client = MedullaClient::new(base, "jwt-abc");
    let err = client.me().await.unwrap_err();
    assert!(err.is_token_expired());
}

// ---------------------------------------------------------------------------
// Integration: full endpoint surface against the TCP stub
// ---------------------------------------------------------------------------

/// Like [`spawn_stub`], but also hands back the raw request bytes the client
/// sent so tests can assert on the method line, query string, and body.
async fn spawn_stub_capture(response: Vec<u8>) -> (String, tokio::sync::oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        sock.write_all(&response).await.unwrap();
        sock.flush().await.unwrap();
        let _ = sock.shutdown().await;
        let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
    });
    (format!("http://{addr}"), rx)
}

/// A loopback address that cannot have a listener, so connecting is refused.
///
/// Reserving an ephemeral port and then releasing it races with the parallel
/// stub tests: another listener can acquire that port before this client
/// connects and return an unrelated response. Port zero is reserved and is
/// never a valid TCP listener destination, avoiding that time-of-check/time-of-use
/// window.
fn dead_addr() -> String {
    "http://127.0.0.1:0".to_string()
}

fn ok_envelope(status: &str, data: serde_json::Value) -> Vec<u8> {
    http_json(
        status,
        &json!({ "success": true, "data": data }).to_string(),
    )
}

#[tokio::test]
async fn consume_login_token_returns_jwt() {
    let (base, req) =
        spawn_stub_capture(ok_envelope("HTTP/1.1 200 OK", json!({ "jwt": "jwt-xyz" }))).await;
    let client = MedullaClient::new(base, "");
    let jwt = client.consume_login_token("one-time").await.unwrap();
    assert_eq!(jwt, "jwt-xyz");
    let sent = req.await.unwrap();
    assert!(sent.starts_with("POST /auth/login-token/consume"), "{sent}");
    assert!(sent.contains("\"token\":\"one-time\""), "{sent}");
}

#[tokio::test]
async fn me_carries_bearer_and_unwraps() {
    let (base, req) =
        spawn_stub_capture(ok_envelope("HTTP/1.1 200 OK", json!({ "sub": "user-1" }))).await;
    let client = MedullaClient::new(base, "jwt-abc");
    let me = client.me().await.unwrap();
    assert_eq!(me["sub"], json!("user-1"));
    let sent = req.await.unwrap();
    assert!(sent.contains("authorization: Bearer jwt-abc"), "{sent}");
}

#[tokio::test]
async fn http_requests_identify_medulla() {
    let (base, req) =
        spawn_stub_capture(ok_envelope("HTTP/1.1 200 OK", json!({ "sub": "user-1" }))).await;
    let client = MedullaClient::new(base, "jwt-abc");
    client.me().await.unwrap();

    let sent = req.await.unwrap();
    assert!(sent.contains("x-sdk-name: medulla"), "{sent}");
    assert!(!sent.contains("x-sdk-name: openhuman"), "{sent}");
}

#[tokio::test]
async fn team_usage_fetches_and_unwraps() {
    let data = json!({
        "plan": "pro",
        "inferenceTotals": { "spent": 1.5, "calls": 3 },
        "remainingUsd": 4.5,
    });
    let (base, req) = spawn_stub_capture(ok_envelope("HTTP/1.1 200 OK", data)).await;
    let client = MedullaClient::new(base, "jwt-abc");
    let usage = client.team_usage().await.unwrap();
    assert_eq!(usage["plan"], json!("pro"));
    assert_eq!(usage["inferenceTotals"]["calls"], json!(3));
    let sent = req.await.unwrap();
    assert!(sent.starts_with("GET /teams/me/usage"), "{sent}");
    assert!(sent.contains("authorization: Bearer jwt-abc"), "{sent}");
}

#[tokio::test]
async fn http_error_without_envelope_becomes_api_error() {
    let (base, _req) = spawn_stub_capture(http_json(
        "HTTP/1.1 503 Service Unavailable",
        "upstream down",
    ))
    .await;
    let client = MedullaClient::new(base, "jwt");
    let err = client.get_session("s1").await.unwrap_err();
    assert_eq!(err.status(), Some(503));
    assert_eq!(err.error_code(), None);
    match err {
        ClientError::Api { message, .. } => assert_eq!(message, "upstream down"),
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
async fn success_status_with_non_json_body_is_decode_error() {
    let (base, _req) = spawn_stub_capture(http_json("HTTP/1.1 200 OK", "not json at all")).await;
    let client = MedullaClient::new(base, "jwt");
    // A typed endpoint: the SDK preserves an unparseable 2xx body as a JSON
    // string, and decoding it into the endpoint's shape is what fails.
    let err = client.get_session("s1").await.unwrap_err();
    assert!(matches!(err, ClientError::Decode(_)), "got {err:?}");
}

#[tokio::test]
async fn connection_refused_surfaces_transport_error() {
    let base = dead_addr();
    let client = MedullaClient::new(base, "jwt");
    let err = client.list_sessions().await.unwrap_err();
    assert!(matches!(err, ClientError::Transport(_)), "got {err:?}");
    assert_eq!(err.status(), None);
    assert_eq!(err.error_code(), None);
}
