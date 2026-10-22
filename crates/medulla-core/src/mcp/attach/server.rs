//! MCP endpoint construction for sessions served by this Medulla process.

use std::path::PathBuf;

use medulla_harness::mcp::{claude_mcp_config, McpEndpoint, McpServerSpec};
use serde_json::{json, Map, Value};

use super::files::{config_file_path, mcp_state_dir, write_owner_only};
use super::{ATTACHED_ENV, SERVER_NAME, TOOL_MODE_ENV, TOOL_SCOPE_ENV};

/// A stdio MCP server, described once and rendered per transport.
///
/// Deliberately not an ACP type: the PTY path has no ACP types in scope, and
/// making the shared description depend on the ACP schema would mean the
/// agreement between the two transports lives in one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    /// The server key a harness lists this under, and the prefix a model sees
    /// in every tool name (`mcp__medulla__workflow_run`).
    pub name: &'static str,
    /// The binary to launch — this same Medulla executable.
    pub command: PathBuf,
    /// Its argv: `mcp`.
    pub args: Vec<String>,
    /// Environment for the server process that carries no secret, so it is safe
    /// to render into a registration a transport may put on a command line.
    pub env: Vec<(String, String)>,
    /// Environment that *is* secret — the fleet grant and the socket it is
    /// redeemed at. Kept apart from [`Self::env`] so a transport cannot leak it
    /// onto an argv by treating all of the server's environment alike.
    pub secret_env: Vec<(String, String)>,
}

impl ServerSpec {
    /// Stamp this server with the session key it is being attached to.
    ///
    /// The tool server is a subprocess of a harness Medulla launched, and it is
    /// the only thing in that process tree that knows which session it belongs
    /// to — the key its grant was minted under. Carrying it lets a workflow the
    /// session starts record who started it, which is what nests the run under
    /// its session in the Agents rail rather than leaving it an anonymous entry
    /// in a global history.
    ///
    /// Non-secret on purpose: it is a correlation key, not a capability. It
    /// authorizes nothing, so a transport that renders it onto an argv leaks
    /// only the fact that a session exists.
    pub fn for_session(mut self, session: &str) -> Self {
        let session = session.trim();
        if !session.is_empty() {
            self.env
                .push((ORIGIN_SESSION_ENV.to_string(), session.to_string()));
        }
        self
    }

    /// The `--mcp-config` document registering this server with Claude Code.
    ///
    /// Only the non-secret environment is rendered; [`Self::secret_env`] reaches
    /// the server by inheritance from the harness process instead. See the
    /// module docs for why.
    ///
    /// Claude Code merges `--mcp-config` with the servers it discovers itself
    /// (`.mcp.json`, user scope) unless `--strict-mcp-config` is also passed,
    /// which is why this adds Medulla's server without displacing the
    /// operator's.
    pub fn claude_mcp_config(&self) -> String {
        claude_mcp_config(&self.render_spec())
    }

    fn render_spec(&self) -> McpServerSpec {
        McpServerSpec {
            name: self.name,
            command: self.command.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        }
    }

    /// Write this server's full registration — [`Self::secret_env`] included —
    /// to a file only its owner can read, and return the path.
    ///
    /// This is how the secret half reaches Claude Code without ever landing on
    /// an argv or the harness's own environment; see the module docs for why
    /// both of those are unsafe for it. `--mcp-config` accepts a file path as
    /// readily as an inline document, and Claude Code applies a server's `env`
    /// only when spawning *that* server — so a grant recorded here reaches the
    /// `medulla mcp` subprocess and nothing else the harness starts.
    ///
    /// Named after `session` so [`revoke_session`] can find and remove it with
    /// no plumbing back from wherever this was called: reconstructing the same
    /// path from the same key is enough.
    ///
    /// # Errors
    ///
    /// `InvalidInput` when `session` is not [`is_safe_session_component`] — a
    /// `/`, a `..`, or an absolute path would otherwise let a caller-supplied
    /// key write this secret outside [`mcp_state_dir`] entirely. Every other
    /// error is whatever creating or writing the file returned. Either way the
    /// caller's fallback is the non-secret inline document — see
    /// [`attach_cli`].
    pub fn write_config_file(&self, session: &str) -> std::io::Result<PathBuf> {
        let path = config_file_path(session).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("session is not a safe filename component: {session:?}"),
            )
        })?;
        let mut entry = Map::new();
        entry.insert(
            "command".to_string(),
            Value::String(self.command.display().to_string()),
        );
        entry.insert(
            "args".to_string(),
            Value::Array(self.args.iter().cloned().map(Value::String).collect()),
        );
        let mut env: Map<String, Value> = self
            .env
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect();
        for (key, value) in &self.secret_env {
            env.insert(key.clone(), Value::String(value.clone()));
        }
        if !env.is_empty() {
            entry.insert("env".to_string(), Value::Object(env));
        }
        let document = json!({ "mcpServers": { self.name: Value::Object(entry) } }).to_string();
        std::fs::create_dir_all(mcp_state_dir())?;
        write_owner_only(&path, &document)?;
        Ok(path)
    }
}

impl McpEndpoint for ServerSpec {
    fn endpoint(&self, session: &str) -> McpServerSpec {
        self.clone().for_session(session).render_spec()
    }
}

/// An explicit path to the binary that serves Medulla's MCP tools.
///
/// Normally there is nothing to set: the server is *this* program run as
/// `medulla mcp`, and [`std::env::current_exe`] names it. That assumption holds
/// for the TUI, the daemon, and the CLI, and breaks for anything that links the
/// SDK without being the Medulla binary — most visibly a `cargo test` harness,
/// where `current_exe` is `target/debug/deps/<suite>-<hash>` and running it with
/// `mcp` on its argv serves nothing at all. The result is not an error but an
/// agent with no tools, which is the failure this whole module is trying to make
/// impossible; an integration test that wants a *real* tool surface points this
/// at a built `medulla`.
///
/// Untrusted configuration, per `AGENTS.md`: whatever it names is executed. It
/// is honoured only when it points at an existing file, and it never widens what
/// a session is served — the grant still decides that.
pub const SERVER_COMMAND_ENV: &str = "MEDULLA_MCP_COMMAND";

/// The harness session a tool server is serving.
///
/// Set by [`ServerSpec::for_session`] on every registration, both transports.
/// Read by the server itself ([`crate::mcp::serve_stdio`]) and stamped onto
/// every workflow run that session starts, so an operator can see which of the
/// harnesses on their rail is responsible for a run.
///
/// Untrusted in the sense that anything in the process tree can rewrite it: it
/// is an attribution hint, never an authorization. Nothing is granted on the
/// strength of it, and the worst a forged value can do is file a run under the
/// wrong session in a list.
pub const ORIGIN_SESSION_ENV: &str = "MEDULLA_ORIGIN_SESSION";

/// Proof that this MCP subprocess was attached by a Medulla launch seam.
///
/// User- and project-level MCP registrations do not carry this value. The
/// stdio server refuses to start without it, so merely discovering an old
/// `medulla mcp` registration cannot give an ordinary harness the workflow
/// surface. This is a provenance guard against accidental ambient discovery,
/// not a privilege boundary against another process running as the same OS
/// user: such a process can inspect and reproduce any child environment.

/// The binary a spawned tool server runs.
///
/// `None` when neither the override nor this process's own path resolves, which
/// leaves the caller no command to register — see [`server_spec`], which
/// attaches nothing rather than a registration the harness would fail to start.
pub fn server_command() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(SERVER_COMMAND_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    std::env::current_exe().ok()
}

/// The server to offer a session, or `None` when no family would be served.
///
/// `fleet` is the grant the caller resolved — locally minted
/// ([`local_fleet_grant`]) or, for ACP, exchanged from a verified parent
/// handoff. With no fleet grant *and* workflows switched off there is nothing
/// left to serve, so no server is attached rather than one that answers every
/// call with a refusal.
///
/// Returns `None` too when this binary's own path cannot be resolved: a
/// registration pointing at a command we could not name would fail in the
/// harness, where the operator has no way to see why.
pub fn server_spec(
    tool_mode: Option<&str>,
    fleet: Option<(PathBuf, String)>,
    workflows_enabled: bool,
) -> Option<ServerSpec> {
    if !workflows_enabled && fleet.is_none() {
        return None;
    }
    let command = server_command()?;
    let mut spec = ServerSpec {
        name: super::SERVER_NAME,
        command,
        args: vec!["mcp".to_string()],
        // A persistent registration has no marker. Only the per-launch spec
        // gets one, which is what leaves ambient registrations inert.
        env: vec![(ATTACHED_ENV.to_string(), uuid::Uuid::new_v4().to_string())],
        secret_env: Vec::new(),
    };
    if let Some((socket, token)) = fleet {
        spec.secret_env.push((
            crate::control_socket::MCP_SOCKET_ENV.to_string(),
            socket.to_string_lossy().into_owned(),
        ));
        spec.secret_env
            .push((crate::control_socket::MCP_GRANT_ENV.to_string(), token));
    }
    // Passed explicitly rather than left to inheritance, because it is the one
    // setting that differs per session: one process serves authoring turns and
    // review turns, so an inherited value could only ever be right for one of
    // them.
    if let Some(mode) = tool_mode {
        let (mode, scope) = mode
            .split_once(':')
            .map_or((mode, None), |(mode, scope)| (mode, Some(scope)));
        spec.env
            .push((super::TOOL_MODE_ENV.to_string(), mode.to_string()));
        if let Some(scope) = scope {
            spec.env
                .push((super::TOOL_SCOPE_ENV.to_string(), scope.to_string()));
        }
    }
    Some(spec)
}
