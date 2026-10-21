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
fn default_shown_boxes() -> String {
    "cpu,mem,net,agents".into()
}
fn default_presets() -> String {
    "cpu:0:default,mem:0:default,net:0:default,agents:1:default".into()
}

fn deserialize_update_ms<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    Ok(value.clamp(MIN_RESOURCES_UPDATE_MS, MAX_RESOURCES_UPDATE_MS))
}

/// Settings for the background resource sampler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourcesConfig {
    /// Milliseconds between system resource samples.
    #[serde(
        default = "default_update_ms",
        deserialize_with = "deserialize_update_ms"
    )]
    pub update_ms: u64,
    /// Comma-separated boxes visible when the tab first opens.
    #[serde(default = "default_shown_boxes")]
    pub shown_boxes: String,
    /// Whitespace-separated btop-style box presets.
    #[serde(default = "default_presets")]
    pub presets: String,
    /// Index of the selected preset in `presets`.
    pub preset: usize,
    /// Selected preset index, zero based.
    pub preset: usize,
    /// Explicit CPU placement override. When absent, the selected preset wins.
    pub cpu_bottom: Option<bool>,
    /// Explicit Mem/Net placement override. When absent, the selected preset wins.
    pub mem_below_net: Option<bool>,
    /// Explicit Agents placement override. When absent, the selected preset wins.
    pub agents_left: Option<bool>,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            update_ms: DEFAULT_RESOURCES_UPDATE_MS,
            shown_boxes: default_shown_boxes(),
            presets: default_presets(),
            preset: 0,
            preset: 0,
            cpu_bottom: None,
            mem_below_net: None,
            agents_left: None,
        }
    }
}

impl ResourcesConfig {
    /// Return the supported interval, clamping programmatic values as well.
    pub fn update_interval_ms(&self) -> u64 {
        self.update_ms
            .clamp(MIN_RESOURCES_UPDATE_MS, MAX_RESOURCES_UPDATE_MS)
    }
}
