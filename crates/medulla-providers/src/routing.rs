//! Provider capability used by session routing and argv construction.

use medulla_types::HarnessProvider;

/// Whether a provider can resume a previously captured harness session id.
pub fn can_resume(provider: HarnessProvider) -> bool {
    matches!(provider, HarnessProvider::Claude | HarnessProvider::Codex)
}
