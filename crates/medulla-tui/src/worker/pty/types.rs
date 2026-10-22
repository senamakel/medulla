//! TUI launch input and backward-compatible PTY data re-exports.

use medulla::protocol::HarnessProvider;
pub use medulla::sessions::SessionOrigin;
pub use medulla_pty::worker::pty::types::{CloseAttempt, PtyState, SessionControl, SessionRow};

/// Default geometry for a freshly opened session.
pub const DEFAULT_COLS: u16 = 120;
/// Default row count for a freshly opened session.
pub const DEFAULT_ROWS: u16 = 30;
/// Number of retained scrollback lines per session.
pub const SCROLLBACK: usize = 2_000;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    /// Which coding-agent CLI to run.
    pub provider: HarnessProvider,
    /// The custom preset this session was launched from, when it was one.
    ///
    /// A preset is a *different agent* running the same CLI — its own model,
    /// endpoint and environment — so its id, not the base CLI's wire name, is
    /// what a declaration records for it. Carrying it here is what lets the rail
    /// file the session under the agent that declared it; without it a
    /// preset-backed session was compared as `claude` against a declaration
    /// saying `deepseek` and listed as belonging to no agent at all.
    ///
    /// `None` for a native CLI entry, whose id *is* the provider's wire name.
    pub preset: Option<String>,
    /// The resolved binary name or path.
    pub bin: String,
    /// Working directory for the child.
    pub cwd: String,
    /// Environment for the child.
    pub env: std::collections::HashMap<String, String>,
    /// Extra argv appended after the provider's interactive base args.
    pub extra_args: Vec<String>,
    /// Whether to launch with the provider's permission-bypass flag.
    ///
    /// A watched session is still unattended: nobody is sitting in the pane to
    /// answer "allow this command?", so a task that stops on one has hung.
    pub skip_permissions: bool,
    /// A label for the session list — the peer's id.
    pub label: String,
    /// An operator- or peer-configured model override, when set.
    ///
    /// Threaded onto the interactive argv the same way the headless path adds
    /// it to its one-shot invocation: without this, a `[host].model` or a
    /// task's own model hint silently reached the harness's own default
    /// instead, changing which model actually ran.
    pub model: Option<String>,
    /// A session id to hand the harness, when it accepts one.
    ///
    /// Claude does (`--session-id`), and the transcript is then written under
    /// that name — so it can be found by identity rather than by guessing which
    /// file is newest. Codex has no such flag; its rollout records its own id on
    /// line one, which the tailer reads back instead.
    pub session_id: Option<String>,
    /// Who holds the session the moment it opens.
    ///
    /// A task frame opens an [`Orchestrator`](SessionControl::Orchestrator)
    /// session; an operator spawning one from the TUI opens a
    /// [`User`](SessionControl::User) one, which is what makes it unmanaged.
    pub control: SessionControl,
    /// Who is starting this session — see [`SessionOrigin`].
    ///
    /// Display and labelling only — never gate behaviour on it. Control is the
    /// gate; this is what lets a session be shown under its agent as "started
    /// by you" rather than the less useful "orchestrator-spawned, currently
    /// yours". It is fixed here for the session's whole life.
    pub origin: SessionOrigin,
    /// The display name the operator gave this session as they spun it up.
    ///
    /// `None` for a dispatched session, which is labelled from its task
    /// instead, and `None` today for an operator-started one as well — the
    /// picker has no name prompt yet. The field is the seam that lets one
    /// appear without touching this layer again.
    pub name: Option<String>,
    /// The key this session's MCP fleet grant was minted under, when one was.
    ///
    /// Carried so the grant can be handed back when the harness exits: a
    /// capability that outlives the session it was minted for is a token
    /// nothing revokes and a leaked subprocess could still redeem.
    pub mcp_grant_session: Option<String>,
}
