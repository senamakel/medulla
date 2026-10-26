//! Data types for agent discovery: process records, known roots, and classified agent processes.

use std::path::PathBuf;

/// A recognized coding agent or supported shell process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentKind {
    /// Claude Code CLI.
    Claude,
    /// Codex CLI.
    Codex,
    /// Codex app-server process.
    CodexAppServer,
    /// Claude Code ACP adapter.
    ClaudeAcp,
    /// Codex ACP adapter.
    CodexAcp,
    /// OpenCode CLI.
    Opencode,
    /// OpenCode ACP adapter.
    OpencodeAcp,
    /// Standalone OpenHuman core process.
    OpenhumanCore,
    /// OpenHuman runtime embedded in Medulla itself.
    EmbeddedOpenhuman,
    /// Gemini CLI.
    Gemini,
    /// Aider.
    Aider,
    /// Goose.
    Goose,
    /// Cursor Agent.
    CursorAgent,
    /// A shell hosting an agent session.
    Shell,
    /// A supported executable whose command-line signature could not be read.
    Unknown,
}

impl AgentKind {
    /// The name an operator reads in a process list.
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
            AgentKind::CodexAppServer => "Codex Server",
            AgentKind::ClaudeAcp => "Claude-ACP",
            AgentKind::CodexAcp => "Codex-ACP",
            AgentKind::Opencode => "OpenCode",
            AgentKind::OpencodeAcp => "OpenCode-ACP",
            AgentKind::OpenhumanCore | AgentKind::EmbeddedOpenhuman => "OpenHuman",
            AgentKind::Gemini => "Gemini",
            AgentKind::Aider => "Aider",
            AgentKind::Goose => "Goose",
            AgentKind::CursorAgent => "Cursor Agent",
            AgentKind::Shell => "Shell",
            AgentKind::Unknown => "Unknown",
        }
    }
}

/// Ownership source for a discovered process tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentOrigin {
    /// Launched to serve the session with this id.
    Session {
        /// Session identifier.
        id: String,
    },
    /// Launched to serve the task with this id.
    Task {
        /// Task identifier.
        id: String,
    },
    /// Launched by Medulla with no session or task attached.
    Headless,
    /// Observed on a remote host.
    Remote {
        /// Name of the host the process runs on.
        host: String,
    },
    /// Started outside Medulla.
    External,
}

/// Resource counters associated with one process.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessUsage {
    /// CPU use as a percentage of one logical CPU.
    pub cpu_percent: f32,
    /// Resident memory in bytes.
    pub rss_bytes: u64,
    /// Number of OS threads.
    pub threads: u64,
    /// Cumulative IO counters, when available.
    pub io: Option<IoCounters>,
}

/// Cumulative process IO counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IoCounters {
    /// Bytes read since process start, if reported.
    pub read_bytes: u64,
    /// Bytes written since process start, if reported.
    pub written_bytes: u64,
}

/// One platform-neutral row from a process sampler.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessRecord {
    /// Operating-system process identifier.
    pub pid: u32,
    /// Parent process identifier, when known.
    pub ppid: Option<u32>,
    /// Path of the executable, when readable.
    pub exe: Option<PathBuf>,
    /// `None` means argv could not be read (for example, macOS privacy rules).
    pub argv: Option<Vec<String>>,
    /// Working directory, when readable.
    pub cwd: Option<PathBuf>,
    /// Process start time in seconds since the Unix epoch.
    pub started_at_secs: u64,
    /// Resource counters for this process.
    pub usage: ProcessUsage,
}

/// A process root whose ownership is known to Medulla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownProcessRoot {
    /// Operating-system process identifier.
    pub pid: u32,
    /// Process start time in seconds since the Unix epoch.
    pub started_at_secs: u64,
    /// Who started the process tree.
    pub origin: AgentOrigin,
}

/// Controls optional agent categories in process discovery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentDiscoveryOptions {
    /// Include external shells in addition to shells owned by known Medulla roots.
    pub show_external_shells: bool,
}

/// A descendant in an agent process tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessNode {
    /// Operating-system process identifier.
    pub pid: u32,
    /// Path of the executable, when readable.
    pub exe: Option<PathBuf>,
    /// Resource counters for this process.
    pub usage: ProcessUsage,
    /// Descendant processes.
    pub children: Vec<ProcessNode>,
}

/// A classified agent process and its aggregate resource usage.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentProcess {
    /// Root process identifier. `None` for an in-process agent such as the
    /// embedded OpenHuman runtime.
    pub pid: Option<u32>,
    /// The harness this process was classified as.
    pub kind: AgentKind,
    /// Who started the process tree.
    pub origin: AgentOrigin,
    /// Path of the executable, when readable.
    pub exe: Option<PathBuf>,
    /// Working directory, when readable.
    pub cwd: Option<PathBuf>,
    /// Process start time in seconds since the Unix epoch.
    pub started_at_secs: u64,
    /// Seconds the process has been running.
    pub runtime_secs: u64,
    /// Resource counters for this process.
    pub usage: ProcessUsage,
    /// Descendant processes.
    pub children: Vec<ProcessNode>,
}
