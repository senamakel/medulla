//! Injectable environment seams for the budget probe.

use medulla_types::HarnessProvider;
use medulla_providers::ConfiguredBudget;

/// Injectable machine and clock access used by the capability probe.
pub struct BudgetSeams {
    /// Whether the provider's binary resolves on this machine.
    pub installed: Box<dyn Fn(HarnessProvider) -> bool + Send + Sync>,
    /// Authentication signal (`None` means unknown).
    pub authenticated: Box<dyn Fn(HarnessProvider) -> Option<bool> + Send + Sync>,
    /// Detected cooldown expiry.
    pub cooldown_until: Box<dyn Fn(HarnessProvider) -> Option<i64> + Send + Sync>,
    /// Operator-configured budget numbers.
    pub configured: Box<dyn Fn(HarnessProvider) -> Option<ConfiguredBudget> + Send + Sync>,
    /// Current time in Unix seconds.
    pub now_unix: Box<dyn Fn() -> i64 + Send + Sync>,
}
