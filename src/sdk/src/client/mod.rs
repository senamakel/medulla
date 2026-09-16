//! Typed Medulla surface over the shared `tinyhumans-sdk` transport.
//!
//! Surfaces: auth (`/auth`, which is also where the plan entitlement the
//! [`access`](crate::access) gate reads comes from), account usage
//! (`/teams/me/usage`), the onboarding history reward
//! (`/agent-integrations/history-rewards`), and the public feedback board
//! (`/feedback`, in [`feedback`]).
//!
//! Every HTTP request is issued by [`tinyhumans_sdk::TinyHumansClient`], which
//! owns credential headers, the `{ "success": true, "data": ... }` envelope,
//! percent-encoding of path segments, and the not-exposed-route gate. Backend
//! failures arrive as [`ClientError::Api`] with the `errorCode` preserved — see
//! the conversion in [`error`].
//!
//! What remains here is the part the shared SDK does not model. Routes it
//! returns as open `DynamicResponse` JSON are decoded into the typed DTOs in
//! [`types`]. A few routes go through [`tinyhumans_sdk::TinyHumansClient::raw`],
//! the SDK's own escape hatch, because this crate models their contract more
//! precisely than the SDK's request types can express — the NDJSON transcript
//! upload, the full history-reward status. Each such call says why inline.
//! They still share the one transport.

pub mod error;
pub mod feedback;
pub mod types;

pub use error::{ClientError, Result};
pub use feedback::{
    FeedbackComment, FeedbackDetail, FeedbackGithub, FeedbackItem, FeedbackPage, FeedbackQuery,
    FeedbackSort, FeedbackStatus, FeedbackSubmission, FeedbackType,
};
pub use types::*;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tinyhumans_sdk::api::types::LoginTokenRequest;
use tinyhumans_sdk::{QueryParam, TinyHumansClient};

/// Default backend base URL.
pub const DEFAULT_BASE_URL: &str = "http://localhost:5000";

/// How long the default HTTP client waits for a TCP/TLS connection.
const DEFAULT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// How long the default HTTP client tolerates a silent socket mid-response.
///
/// An idle timeout rather than a whole-request one, long enough for the
/// multipart history upload on a slow uplink.
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Header through which the backend attributes traffic to a TinyHumans product.
const PRODUCT_IDENTITY_HEADER: &str = "x-sdk-name";

/// Product identity reported by this client for backend attribution.
const PRODUCT_IDENTITY: &str = "medulla";

/// The HTTP client used when the caller supplies none.
///
/// `reqwest::Client::default()` has no timeouts at all, so a black-holed
/// connection hangs sign-in — or any other call — forever. Falls back to the
/// untimed default only if the builder fails, which it does not for this
/// configuration.
fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .read_timeout(DEFAULT_READ_TIMEOUT)
        .build()
        .unwrap_or_default()
}

impl MedullaClientBuilder {
    /// Set the backend base URL (default [`DEFAULT_BASE_URL`]).
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the bearer JWT sent with every request.
    pub fn jwt(mut self, jwt: impl Into<String>) -> Self {
        self.jwt = Some(jwt.into());
        self
    }

    /// Supply a preconfigured `reqwest::Client`.
    ///
    /// It is handed to the SDK as well, so ordinary requests and the SSE stream
    /// share one connection pool, TLS backend, and proxy policy.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Build the client.
    pub fn build(self) -> MedullaClient {
        let base_url = self
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let jwt = self.jwt.unwrap_or_default();
        let http = self.http.unwrap_or_else(default_http_client);
        let default_headers = product_identity_headers();
        MedullaClient {
            sdk: build_sdk(&base_url, &jwt, http.clone(), default_headers.clone()),
            base_url,
            jwt,
            http,
            default_headers,
        }
    }
}

/// Construct the SDK client that backs a [`MedullaClient`].
///
/// The host's `reqwest::Client` is passed in rather than letting the SDK build
/// its own, so both halves of this client share one transport policy.
fn build_sdk(
    base_url: &str,
    jwt: &str,
    http: reqwest::Client,
    default_headers: HeaderMap,
) -> TinyHumansClient {
    TinyHumansClient::new(base_url)
        .with_http_client(http)
        .with_default_headers(default_headers)
        .with_token(Some(jwt.to_string()))
}

/// Headers shared by the SDK transport and the hand-rolled SSE transport.
///
/// Constructed once with [`MedullaClient`], so every backend request reports
/// the same product identity regardless of its transport.
fn product_identity_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(PRODUCT_IDENTITY_HEADER),
        HeaderValue::from_static(PRODUCT_IDENTITY),
    );
    headers
}

/// Decode an SDK response body into one of this crate's typed DTOs.
fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| ClientError::Decode(e.to_string()))
}

impl MedullaClient {
    /// Start building a client.
    pub fn builder() -> MedullaClientBuilder {
        MedullaClientBuilder::default()
    }

    /// Construct a client from a base URL and JWT.
    pub fn new(base_url: impl Into<String>, jwt: impl Into<String>) -> Self {
        Self::builder().base_url(base_url).jwt(jwt).build()
    }

    /// The configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The configured JWT.
    pub fn jwt(&self) -> &str {
        &self.jwt
    }

    /// Replace the JWT (e.g. after a token refresh).
    ///
    /// Rebuilds the SDK client, which captures the credential at construction.
    pub fn set_jwt(&mut self, jwt: impl Into<String>) {
        self.jwt = jwt.into();
        self.sdk = build_sdk(
            &self.base_url,
            &self.jwt,
            self.http.clone(),
            self.default_headers.clone(),
        );
    }

    /// Issue a request through the SDK's raw transport and decode the unwrapped
    /// payload.
    ///
    /// Used only where this crate's request or response contract is finer than
    /// the SDK's typed namespace method; every caller documents which.
    async fn raw<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[QueryParam],
        body: Option<&Value>,
    ) -> Result<T> {
        decode(self.sdk.raw().send(method, path, query, body, true).await?)
    }

    // --- Auth ------------------------------------------------------------

    /// Exchange a one-time login token for a JWT
    /// (`POST /auth/login-token/consume`).
    pub async fn consume_login_token(&self, token: impl Into<String>) -> Result<String> {
        let response = self
            .sdk
            .auth()
            .consume_login_token(&LoginTokenRequest {
                token: token.into(),
            })
            .await?;
        let out: LoginTokenResult = decode(response.0)?;
        Ok(out.jwt)
    }

    /// Fetch the authenticated principal (`GET /auth/me`).
    pub async fn me(&self) -> Result<Value> {
        Ok(self.sdk.auth().me().await?.0)
    }

    /// Account-level token usage for the active team/user
    /// (`GET /teams/me/usage`): cycle window, spend, per-model breakdown,
    /// remaining budget, plan. Returned as raw JSON — the shape is rendered
    /// defensively by the UI.
    pub async fn team_usage(&self) -> Result<Value> {
        Ok(self.sdk.teams().get_my_usage().await?.0)
    }

    // --- History rewards -------------------------------------------------

    /// Upload one redacted session transcript toward the onboarding reward
    /// (`POST /agent-integrations/history-rewards/uploads`).
    ///
    /// The caller is responsible for redacting `content` first — see
    /// [`crate::history_upload`]. The backend encrypts what it receives before
    /// storing it, and returns the claim's running metrics.
    ///
    /// Errors with [`ClientError::Api`] when the reward was already claimed, the
    /// session cap is reached, or the transcript exceeds the size limit.
    pub async fn upload_history_session(
        &self,
        agent: &str,
        content: String,
    ) -> Result<HistoryUploadResult> {
        // Raw multipart rather than `agent_integrations().upload_history`: that
        // helper sends the file part without a content type, and this upload
        // declares NDJSON.
        let part = reqwest::multipart::Part::text(content)
            .file_name("session.jsonl")
            .mime_str("application/x-ndjson")?;
        let form = reqwest::multipart::Form::new()
            .text("agent", agent.to_string())
            .part("file", part);

        decode(
            self.sdk
                .raw()
                .post_multipart("/agent-integrations/history-rewards/uploads", form)
                .await?,
        )
    }

    /// Score the uploaded history and grant the welcome credit
    /// (`POST /agent-integrations/history-rewards/claim`).
    ///
    /// Idempotent: a repeat call returns the same award with
    /// `already_claimed` set rather than granting again.
    pub async fn claim_history_reward(&self) -> Result<HistoryRewardClaim> {
        // Raw rather than `agent_integrations().claim_history_reward`: the SDK's
        // response type is a three-field projection, while the reveal screen
        // needs the settled status and the per-metric breakdown.
        self.raw(
            Method::POST,
            "/agent-integrations/history-rewards/claim",
            &[],
            None,
        )
        .await
    }

    /// Whether the caller has already earned the history reward
    /// (`GET /agent-integrations/history-rewards/status`).
    ///
    /// A user who has never uploaded gets a zeroed, unclaimed status rather
    /// than an error.
    pub async fn history_reward_status(&self) -> Result<HistoryRewardStatus> {
        // Raw for the same reason as `claim_history_reward`: the SDK's
        // `HistoryRewardsStatus` omits the tier, totals, and award this renders.
        self.raw(
            Method::GET,
            "/agent-integrations/history-rewards/status",
            &[],
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
