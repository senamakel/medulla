//! Configuration values for token and subscription meters.

mod subscriptions_types;

pub use subscriptions_types::{
    MeterId, MeterSource, SubscriptionsConfig, DEFAULT_REFRESH_SECONDS, MIN_REFRESH_SECONDS,
};

use serde::{Deserialize, Serialize};

/// Formats available for one local-process resource in the TUI status line.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResourceDisplay {
    /// Do not render this resource.
    #[default]
    Off,
    /// Render the resource as a percentage.
    Percent,
    /// Render the resource's native value, such as bytes or bytes per second.
    Value,
    /// Render the resource as a compact bar.
    Bar,
}
