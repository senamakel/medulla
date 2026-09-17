//! Error types for the OpenPanel transport.

/// A failed OpenPanel request.
///
/// Analytics callers intentionally treat this as non-fatal; retaining the
/// error type keeps the transport independently testable and observable.
#[derive(Debug, thiserror::Error)]
#[error("OpenPanel request failed: {0}")]
pub struct AnalyticsError(#[from] pub reqwest::Error);
