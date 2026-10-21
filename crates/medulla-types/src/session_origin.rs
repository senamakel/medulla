//! Stable origin value shared by SDK and terminal session layers.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Who started a harness session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionOrigin {
    /// Started to serve a dispatched task.
    Orchestrator,
    /// Started by a person from the UI.
    User,
}

impl SessionOrigin {
    /// The wire/display string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::User => "user",
        }
    }
    /// Whether a person started this session.
    pub fn is_user(self) -> bool {
        matches!(self, Self::User)
    }
    /// Whether a dispatch started this session.
    pub fn is_orchestrator(self) -> bool {
        matches!(self, Self::Orchestrator)
    }
}

impl fmt::Display for SessionOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
