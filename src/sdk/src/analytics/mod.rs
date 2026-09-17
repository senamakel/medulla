//! Privacy-conscious product analytics sent to Medulla's OpenPanel project.
//!
//! The terminal application has no browser SDK, so this module sends the
//! documented OpenPanel HTTP payload directly. It deliberately records only
//! lifecycle events and the authenticated account id: prompts, tokens,
//! workspace paths, configuration, and command arguments never enter this
//! boundary. Set [`DISABLED_ENV`] to opt out before Medulla starts.

mod types;

#[cfg(test)]
mod tests;

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;

pub use types::AnalyticsError;

/// The environment variable that disables all OpenPanel requests when set to a
/// truthy value (`1`, `true`, or `yes`, case-insensitively).
pub const DISABLED_ENV: &str = "MEDULLA_ANALYTICS_DISABLED";

/// OpenPanel's ingestion API for the Medulla project.
const API_URL: &str = "https://panel.tinyhumans.ai/api";
/// The public OpenPanel client id for the Medulla project.
const CLIENT_ID: &str = "781d9ce2-62ec-4059-a093-152c88400576";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Identify an authenticated account and record that the application started.
///
/// The request is best-effort: telemetry must not keep a user from opening the
/// app, so failures are intentionally returned for callers to discard.
pub async fn record_application_started(user_id: &str) -> Result<(), AnalyticsError> {
    set_profile(user_id);
    let Some(tracker) = Tracker::from_environment() else {
        return Ok(());
    };
    tracker.identify(user_id).await?;
    tracker.track("application_started", user_id).await
}

/// Identify an account and record a completed sign-in.
pub async fn record_sign_in(user_id: &str) -> Result<(), AnalyticsError> {
    set_profile(user_id);
    let Some(tracker) = Tracker::from_environment() else {
        return Ok(());
    };
    tracker.identify(user_id).await?;
    tracker.track("signed_in", user_id).await
}

/// Record a sign-out before the local credential is removed.
pub async fn record_sign_out(user_id: &str) -> Result<(), AnalyticsError> {
    let Some(tracker) = Tracker::from_environment() else {
        return Ok(());
    };
    let result = tracker.track("signed_out", user_id).await;
    clear_profile();
    result
}

/// Record a top-level TUI screen view for the current authenticated profile.
pub async fn record_screen_view(screen: &str) -> Result<(), AnalyticsError> {
    record_current("screen_viewed", json!({ "screen": screen })).await
}

/// Record a normalized TUI interaction name for the current profile.
pub async fn record_ui_action(action: &str) -> Result<(), AnalyticsError> {
    record_current("ui_action", json!({ "action": action })).await
}

/// A narrow OpenPanel HTTP client used only for Medulla lifecycle events.
#[derive(Clone)]
struct Tracker {
    endpoint: String,
    client: reqwest::Client,
}

impl Tracker {
    /// Construct the tracker unless the user explicitly opted out.
    fn from_environment() -> Option<Self> {
        if analytics_disabled() {
            return None;
        }

        let mut headers = HeaderMap::new();
        headers.insert("openpanel-client-id", HeaderValue::from_static(CLIENT_ID));
        headers.insert("openpanel-sdk-name", HeaderValue::from_static("medulla"));
        headers.insert(
            "openpanel-sdk-version",
            HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .ok()?;
        Some(Self {
            endpoint: format!("{API_URL}/track"),
            client,
        })
    }

    /// Send OpenPanel's `identify` payload without profile attributes that
    /// could expose personal data.
    async fn identify(&self, user_id: &str) -> Result<(), AnalyticsError> {
        self.send(identify_payload(user_id)).await
    }

    /// Send a named lifecycle event associated with the authenticated profile.
    async fn track(&self, name: &str, user_id: &str) -> Result<(), AnalyticsError> {
        self.track_with_properties(name, user_id, json!({})).await
    }

    /// Send a lifecycle event with an explicitly allowlisted property map.
    async fn track_with_properties(
        &self,
        name: &str,
        user_id: &str,
        properties: serde_json::Value,
    ) -> Result<(), AnalyticsError> {
        self.send(track_payload(name, user_id, properties)).await
    }

    /// Submit a payload and turn non-success responses into an error callers
    /// can safely ignore without accidentally treating a rejected event as sent.
    async fn send(&self, payload: serde_json::Value) -> Result<(), AnalyticsError> {
        self.client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Send an event only when this process has a backend-verified profile.
async fn record_current(name: &str, properties: serde_json::Value) -> Result<(), AnalyticsError> {
    let Some(user_id) = current_profile() else {
        return Ok(());
    };
    let Some(tracker) = Tracker::from_environment() else {
        return Ok(());
    };
    tracker
        .track_with_properties(name, &user_id, properties)
        .await
}

/// Process-local identity for events that originate below the TUI wiring.
fn profile_slot() -> &'static Mutex<Option<String>> {
    static PROFILE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    PROFILE.get_or_init(|| Mutex::new(None))
}

/// Record the already verified account identity for this process.
fn set_profile(user_id: &str) {
    *profile_slot().lock().expect("analytics profile lock") = Some(user_id.to_string());
}

/// Read the verified process-local identity, if this is an authenticated run.
fn current_profile() -> Option<String> {
    profile_slot()
        .lock()
        .expect("analytics profile lock")
        .clone()
}

/// Drop the profile as part of logout so a later event cannot be attributed.
fn clear_profile() {
    *profile_slot().lock().expect("analytics profile lock") = None;
}

/// Whether the operator disabled product analytics for this process.
fn analytics_disabled() -> bool {
    std::env::var(DISABLED_ENV)
        .ok()
        .is_some_and(|value| is_truthy(&value))
}

/// Interpret the deliberately small set of values that opt out of telemetry.
fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

/// Build the profile-only identify request used at account boundaries.
fn identify_payload(user_id: &str) -> serde_json::Value {
    json!({ "type": "identify", "payload": { "profileId": user_id } })
}

/// Build a lifecycle event without application content or machine information.
fn track_payload(name: &str, user_id: &str, properties: serde_json::Value) -> serde_json::Value {
    let mut properties = properties.as_object().cloned().unwrap_or_default();
    properties.insert("app".into(), json!("medulla"));
    properties.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
    json!({
        "type": "track",
        "payload": {
            "name": name,
            "profileId": user_id,
            "properties": properties
        }
    })
}
