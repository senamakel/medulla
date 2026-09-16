//! Decode fixtures: success/error envelope unwrapping and error mapping.

use crate::client::*;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// SDK error mapping
// ---------------------------------------------------------------------------
//
// Envelope unwrapping itself now belongs to `tinyhumans-sdk`. What this crate
// still owns is the translation of the SDK's error taxonomy into [`ClientError`],
// which is what front ends branch on — so these cover that seam directly.

/// A failed non-2xx response, as the SDK reports it.
fn sdk_status(status: u16, body: Value) -> ClientError {
    tinyhumans_sdk::Error::Status { status, body }.into()
}

#[test]
fn maps_error_envelope_with_code() {
    let err = sdk_status(
        401,
        json!({ "success": false, "error": "token expired", "errorCode": "TOKEN_EXPIRED" }),
    );
    assert_eq!(err.error_code(), Some("TOKEN_EXPIRED"));
    assert!(err.is_token_expired());
    assert_eq!(err.status(), Some(401));
    match err {
        ClientError::Api { message, .. } => assert_eq!(message, "token expired"),
        other => panic!("expected api error, got {other:?}"),
    }
}

#[test]
fn maps_error_envelope_with_details() {
    let err = sdk_status(
        409,
        json!({
            "success": false,
            "error": "bad",
            "errorCode": "PROTOCOL_MISMATCH",
            "details": { "min": 1, "max": 2 },
        }),
    );
    match err {
        ClientError::Api { details, .. } => {
            let d = details.unwrap();
            assert_eq!(d["min"], json!(1));
            assert_eq!(d["max"], json!(2));
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[test]
fn non_json_error_body_becomes_api_error() {
    // A non-JSON body reaches the SDK error as the raw text.
    let err = sdk_status(500, json!("internal error"));
    assert_eq!(err.status(), Some(500));
    assert_eq!(err.error_code(), None);
    match err {
        ClientError::Api { message, .. } => assert_eq!(message, "internal error"),
        other => panic!("expected api error, got {other:?}"),
    }
}

#[test]
fn error_body_without_a_message_falls_back_to_the_status() {
    let err = sdk_status(502, json!({ "unexpected": true }));
    match err {
        ClientError::Api { message, .. } => assert_eq!(message, "request failed with status 502"),
        other => panic!("expected api error, got {other:?}"),
    }
}

/// The backend does not always pair a failed operation with a non-2xx status.
/// The SDK reports that case separately, and it has to classify the same way.
#[test]
fn unsuccessful_envelope_on_a_2xx_maps_to_an_api_error() {
    let err: ClientError = tinyhumans_sdk::Error::Envelope {
        error: "token expired".into(),
        error_code: Some("TOKEN_EXPIRED".into()),
        details: Value::Null,
    }
    .into();
    assert!(err.is_token_expired());
    assert!(err.is_auth_error());
    // No HTTP status said so, so none is reported.
    assert_eq!(err.status(), None);
    match err {
        ClientError::Api {
            message, details, ..
        } => {
            assert_eq!(message, "token expired");
            assert!(details.is_none(), "a null details payload is dropped");
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

/// A route the SDK intentionally does not expose fails before the network, so
/// it carries no status to misclassify as an auth failure.
#[test]
fn blocked_route_maps_to_a_statusless_api_error() {
    let err: ClientError =
        tinyhumans_sdk::Error::RouteNotExposed("GET".into(), "/coupons/admin".into()).into();
    assert_eq!(err.status(), None);
    assert_eq!(err.error_code(), None);
    assert!(!err.is_auth_error());
}

#[test]
fn socket_failures_keep_their_transport_classification() {
    let err: ClientError = tinyhumans_sdk::Error::MissingSocketToken.into();
    assert!(matches!(err, ClientError::Socket(_)), "got {err:?}");
    assert!(!err.is_auth_error());
}

#[test]
fn auth_error_classification_covers_status_codes_and_non_api_errors() {
    let api_error = |status: Option<u16>, error_code: Option<&str>| ClientError::Api {
        status,
        message: "rejected".into(),
        error_code: error_code.map(str::to_owned),
        details: None,
    };

    assert!(api_error(Some(401), None).is_auth_error());
    assert!(api_error(Some(403), None).is_auth_error());
    assert!(api_error(Some(500), Some("TOKEN_EXPIRED")).is_auth_error());
    assert!(!api_error(Some(400), Some("INVALID_REQUEST")).is_auth_error());

    let decode = ClientError::Decode("bad response".into());
    assert_eq!(decode.error_code(), None);
    assert_eq!(decode.status(), None);
    assert!(!decode.is_token_expired());
    assert!(!decode.is_auth_error());
}

// ---------------------------------------------------------------------------
// LoopEvent / run result decode
// ---------------------------------------------------------------------------
