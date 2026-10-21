//! Resource sampler timing configuration.

use serde::{Deserialize, Serialize};

/// Default resource sample interval, in milliseconds.
pub const DEFAULT_RESOURCES_UPDATE_MS: u64 = 2_000;
/// Smallest supported resource sample interval, in milliseconds.
pub const MIN_RESOURCES_UPDATE_MS: u64 = 100;
/// Largest supported resource sample interval, in milliseconds.
pub const MAX_RESOURCES_UPDATE_MS: u64 = 10_000;

fn default_update_ms() -> u64 {
    DEFAULT_RESOURCES_UPDATE_MS
}

fn deserialize_update_ms<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    Ok(value.clamp(MIN_RESOURCES_UPDATE_MS, MAX_RESOURCES_UPDATE_MS))
}

/// Settings for the background resource sampler.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourcesConfig {
    /// Milliseconds between system resource samples.
    #[serde(
        default = "default_update_ms",
        deserialize_with = "deserialize_update_ms"
    )]
    pub update_ms: u64,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            update_ms: DEFAULT_RESOURCES_UPDATE_MS,
        }
    }
}

impl ResourcesConfig {
    /// Return the supported interval, clamping programmatic values as well.
    pub fn update_interval_ms(self) -> u64 {
        self.update_ms
            .clamp(MIN_RESOURCES_UPDATE_MS, MAX_RESOURCES_UPDATE_MS)
    }
}
