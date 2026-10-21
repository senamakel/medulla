//! Operator-declared and gathered budget input values.

use medulla_types::{BudgetWindow, HarnessProvider};

use crate::ProviderBudgetConfig;

/// Operator-declared budget numbers for a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfiguredBudget {
    /// Opaque seat identifier, never a credential.
    pub seat: Option<String>,
    /// The metering window.
    pub window: Option<BudgetWindow>,
    /// Configured allowance for the window.
    pub limit_tokens: Option<i64>,
    /// Consumption recorded so far.
    pub used_tokens: Option<i64>,
    /// Explicit remaining allowance.
    pub remaining_tokens: Option<i64>,
    /// Unix seconds until which the seat is parked.
    pub cooldown_until: Option<i64>,
}

impl ConfiguredBudget {
    /// Copy the operator's config values without deriving or inventing fields.
    pub fn from_config(cfg: &ProviderBudgetConfig) -> Self {
        Self {
            seat: cfg.seat.clone(),
            window: cfg.window,
            limit_tokens: cfg.limit_tokens,
            used_tokens: cfg.used_tokens,
            remaining_tokens: cfg.remaining_tokens,
            cooldown_until: cfg.cooldown_until,
        }
    }
}

/// Facts already gathered for a provider, supplied to the pure budget evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProbeInput {
    /// Provider being evaluated.
    pub provider: HarnessProvider,
    /// Whether the provider binary is installed.
    pub installed: bool,
    /// Authentication state: unknown is treated as usable.
    pub authenticated: Option<bool>,
    /// Detected cooldown expiry, if any.
    pub cooldown_until: Option<i64>,
    /// Operator-configured numbers, if available.
    pub configured: Option<ConfiguredBudget>,
    /// Current time in Unix seconds.
    pub now_unix: i64,
}
