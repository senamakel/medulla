//! Data passed across the btop embedding boundary.

/// One coding-agent process to show in btop's agents box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTag {
    /// Operating-system process id of the agent's root process.
    pub pid: u32,
    /// Short name shown in the Agent column, such as `claude`.
    pub label: String,
    /// Whether Medulla launched this process (a session or task it owns).
    pub launched: bool,
}

/// Why btop could not be started.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    /// This build has no btop: the target has no btop collector.
    #[error("btop is not available on this platform")]
    Unsupported,
    /// btop was already started in this process; its globals allow one run.
    #[error("btop is already running in this process")]
    AlreadyStarted,
    /// btop's thread could not be created.
    #[error("could not start btop's thread")]
    Spawn,
    /// The config directory path cannot cross the C boundary.
    #[error("btop config path contains a NUL byte")]
    BadPath,
}
