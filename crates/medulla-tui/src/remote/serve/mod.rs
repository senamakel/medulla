//! Serving remote sessions: the daemon half of a mosh-style remote host.
//!
//! A client bootstraps this over SSH, then speaks to it directly over UDP. What
//! it asks for is ordinary: start a session, show me its screen, here are my
//! keystrokes. What is unusual is only where the pty lives.
//!
//! # Why this is in the app crate
//!
//! Because `PtyManager`, `vt100` and `portable-pty` are, and deliberately: the
//! SDK owns the wire model and the link, the app crate owns the emulator and the
//! translation between them. A relay is exactly that translation with a socket
//! on one end, so it belongs on this side of the line. The `medulla` binary is
//! this crate's, so `medulla daemon --direct` costs nothing to reach.
//!
//! # What it reuses, and why that matters more than what it adds
//!
//! Almost everything. Sessions are opened through
//! [`LocalSessions::open_unmanaged_named`] — the *same* call the operator's own
//! picker makes — so a remote coding agent gets MCP registration, managed
//! skills, router injection and commit attribution identically to a local one,
//! and a remote shell gets deliberately none of them. Screens are sampled by the
//! existing [`SessionStream`], which already turns an emulator into a bounded
//! frame stream. Nothing here re-implements either, so there is no second
//! behaviour to drift.
//!
//! [`LocalSessions::open_unmanaged_named`]: crate::ui::harness_pane::LocalSessions::open_unmanaged_named
//! [`SessionStream`]: crate::worker::stream::SessionStream

mod bootstrap;
pub mod control;
pub mod entry;
mod sessions;
mod types;

// Unix only, and not for want of portability: these drive a **real pty running
// a real POSIX shell**, which is the whole of their value — they prove the relay
// against a live child rather than a stand-in. `/bin/sh` and `/tmp` do not exist
// on Windows, so there is nothing there for them to be a test *of*. Making them
// portable would mean replacing the shell with a fake, which is the one thing
// that would stop them catching what they are here to catch.
#[cfg(all(test, unix))]
mod tests;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use medulla::bridge::Bridge;
use medulla::daemon::LogFn;
use medulla::protocol::{
    encode_remote_message, encode_screen_message, parse_remote_message, parse_screen_message,
    RemoteCapabilities, RemoteMessage, RemoteSessionRow, ScreenMessage,
};

use crate::ui::harness_pane::LocalSessions;
use crate::worker::pty::PtyManager;

pub use bootstrap::{
    bind_address, client_dir, detach_from_ssh, enroll_client, hold_bootstrap, instance_path,
    join_endpoint, link_dir, running_instance, unrouted_placeholder, write_instance, BootstrapLock,
    ConnectLine, Instance, CONNECT_SENTINEL,
};
pub use sessions::{resolve_choice, wire_choice, wire_row};
pub use types::Subscriptions;

/// How often the serve loop samples subscribed screens.
///
/// The ceiling the sampler already enforces, expressed as a period. A faster
/// tick would cost sends without producing frames — `SessionStream::tick`
/// returns `None` for an unchanged screen, which is why a watched-but-idle
/// session costs nothing.
const TICK: Duration = Duration::from_millis(100);

/// One remote host's serving loop.
///
/// Owns the sessions it started and the subscriptions watching them; dropping it
/// leaves the sessions running, because a client that goes away is expected to
/// come back — that is the whole point of the transport underneath.
pub struct RemoteServer {
    /// Everything needed to start a session here, exactly as the local picker
    /// would.
    sessions: LocalSessions,
    /// The transport to every connected client.
    bridge: Arc<dyn Bridge>,
    /// Who is watching what.
    subscriptions: Subscriptions,
    /// This host's answer to `Hello`.
    capabilities: RemoteCapabilities,
    /// Every peer that has said `Hello`, so a row change found outside a
    /// request/response pair (a child exiting on its own, an attention cue
    /// firing) still has somewhere to go. Not the same set as
    /// [`Subscriptions`]: a client watching nothing is still owed session-list
    /// updates.
    peers: HashSet<String>,
    /// The session list as last published, so [`publish_sessions_if_changed`]
    /// can tell a real change from another identical tick.
    ///
    /// [`publish_sessions_if_changed`]: Self::publish_sessions_if_changed
    last_rows: Vec<RemoteSessionRow>,
    /// Host process sampler and bounded per-session CPU history for usage rows.
    resource_source: medulla_monitor::sampler::SystemResourceSource,
    usage_sampled_at: Option<Instant>,
    usage_interval: Duration,
    usage_history: std::collections::HashMap<(u32, u64), Vec<f64>>,
    usage_cache: std::collections::HashMap<String, medulla::protocol::AgentUsage>,
    log: Option<LogFn>,
}

impl RemoteServer {
    /// Build a server over the sessions this host can start.
    pub fn new(sessions: LocalSessions, bridge: Arc<dyn Bridge>, host_name: String) -> Self {
        let capabilities = RemoteCapabilities {
            version: env!("CARGO_PKG_VERSION").to_string(),
            harnesses: sessions.choices().iter().map(wire_choice).collect(),
            workspace: sessions.workspace.clone(),
            workspaces: Vec::new(),
            host_name,
        };
        RemoteServer {
            sessions,
            bridge,
            subscriptions: Subscriptions::default(),
            capabilities,
            peers: HashSet::new(),
            last_rows: Vec::new(),
            resource_source: medulla_monitor::sampler::SystemResourceSource::default(),
            usage_sampled_at: None,
            usage_interval: Duration::from_millis(2_000),
            usage_history: std::collections::HashMap::new(),
            usage_cache: std::collections::HashMap::new(),
            log: None,
        }
    }

    /// Offer `workspaces` on the client's workspace step.
    pub fn with_workspaces(mut self, workspaces: Vec<String>) -> Self {
        self.capabilities.workspaces = workspaces;
        self
    }

    /// Set the minimum time between process usage samples and broadcasts.
    pub fn with_usage_interval(mut self, interval: Duration) -> Self {
        self.usage_interval = interval.max(Duration::from_millis(1));
        self
    }

    /// Narrate what the server does to `log`.
    pub fn with_log(mut self, log: LogFn) -> Self {
        self.log = Some(log);
        self
    }

    fn log(&self, line: &str) {
        if let Some(log) = &self.log {
            log(line);
        }
    }

    /// The pty manager backing this host's sessions.
    pub fn pty(&self) -> &PtyManager {
        &self.sessions.sessions
    }

    /// How many clients are watching a screen.
    pub fn watching(&self) -> usize {
        self.subscriptions.len()
    }

    /// What this host advertises, for tests and for the bootstrap's banner.
    pub fn capabilities(&self) -> &RemoteCapabilities {
        &self.capabilities
    }

    /// Which session `peer` is watching, if any.
    pub fn watching_session(&self, peer: &str) -> Option<&str> {
        self.subscriptions.session_for(peer)
    }

    /// Drain and handle whatever has arrived, without sampling or sleeping.
    ///
    /// The other half of one [`run`](Self::run) pass, exposed for the same
    /// reason [`publish_screens_once`](Self::publish_screens_once) is: a caller
    /// driving the server by hand needs to step it rather than start a loop.
    pub async fn pump_once(&mut self) {
        for message in self.bridge.drain_inbox(64).await {
            self.handle(&message.from, &message.text).await;
        }
    }

    /// Sample subscribed screens once and send whatever changed.
    ///
    /// [`run`](Self::run) does this on every pass; exposed separately so a
    /// caller driving the server by hand — a test, or an embedding that owns its
    /// own timer — can step it without a loop.
    pub async fn publish_screens_once(&mut self) {
        self.publish_screens().await;
    }

    /// Publish the session list to every peer, but only when it actually
    /// changed since the last publish.
    ///
    /// [`handle_remote`](Self::handle_remote) already republishes on `Hello`,
    /// `Open`, `Close` and `Kill`, because those are exactly the requests that
    /// change a row. But a row also changes with no request behind it at all: a
    /// child exits on its own, or its busy/working/attention/title/bracketed-
    /// paste state flips mid-run. Without this, a client that is not the one
    /// who caused the change never hears about it — an exited shell keeps
    /// showing as running, and a permission prompt never surfaces. Called every
    /// tick from [`run`](Self::run), so a state change is never more than one
    /// tick old.
    pub async fn publish_sessions_if_changed(&mut self) {
        let rows = self.current_rows();
        if rows == self.last_rows {
            return;
        }
        self.last_rows = rows;
        let peers: Vec<String> = self.peers.iter().cloned().collect();
        for peer in peers {
            self.send_sessions(&peer).await;
        }
    }

    /// Run until the bridge stops producing messages.
    ///
    /// Three things happen on every pass, and the first two are deliberately
    /// independent: inbound control and input are drained, then every
    /// subscribed screen is sampled. Input goes up the reliable channel and
    /// frames come down the latest-wins one, so neither waits on the other —
    /// which is mosh's model and the reason a burst of output cannot delay a
    /// keystroke. The session list is checked last, once whatever this pass did
    /// has had its effect.
    pub async fn run(&mut self) {
        loop {
            for message in self.bridge.drain_inbox(64).await {
                self.handle(&message.from, &message.text).await;
            }
            self.publish_screens().await;
            self.publish_sessions_if_changed().await;
            tokio::time::sleep(TICK).await;
        }
    }

    /// Route one inbound body to whichever protocol claims it.
    ///
    /// Both protocols ride channel 0 and arrive here together; each parser
    /// declines the other's bodies, so the order of these two attempts does not
    /// matter and an unrecognised body is dropped rather than guessed at.
    pub async fn handle(&mut self, from: &str, body: &str) {
        // Recorded on every message rather than only on `Hello`: `Hello` is the
        // handshake, but a peer that reconnects with a fresh transport and skips
        // straight to `Subscribe` is still a peer this daemon must keep the
        // session list current for.
        self.peers.insert(from.to_string());
        if let Some(message) = parse_remote_message(body) {
            self.handle_remote(from, message).await;
            return;
        }
        if let Some(message) = parse_screen_message(body) {
            self.handle_screen(from, message).await;
        }
    }

    /// Session control: hello, open, close.
    async fn handle_remote(&mut self, from: &str, message: RemoteMessage) {
        match message {
            RemoteMessage::Hello { client_version } => {
                self.log(&format!(
                    "remote: {from} connected (client {client_version})"
                ));
                self.send(
                    from,
                    &encode_remote_message(&RemoteMessage::Capabilities(self.capabilities.clone())),
                )
                .await;
                self.publish_sessions(from).await;
            }
            RemoteMessage::Open {
                request_id,
                harness,
                workspace,
                cols,
                rows,
                name,
            } => {
                let answer = match resolve_choice(&self.sessions, &harness) {
                    // Resolved against what this host advertised, never rebuilt
                    // from what the client sent — so a client can only ever
                    // start something already on offer here.
                    None => RemoteMessage::OpenFailed {
                        request_id,
                        reason: format!("this host does not offer {}", harness.id),
                    },
                    Some(choice) => {
                        match self.sessions.open_unmanaged_named(
                            &choice, &workspace,
                            // A remote session is attended: somebody is sitting
                            // in the pane it renders into. So it keeps its
                            // harness's own guardrails, exactly as a local
                            // operator-started session does.
                            false, name,
                        ) {
                            Ok(session_id) => {
                                // Sized before the first frame, so the harness
                                // draws into the pane it will actually live in
                                // rather than starting at a default and
                                // reflowing once.
                                self.sessions.sessions.resize(&session_id, cols, rows);
                                self.log(&format!(
                                    "remote: {from} opened {session_id} ({}) in {workspace}",
                                    choice.display_name()
                                ));
                                RemoteMessage::Opened {
                                    request_id,
                                    session_id,
                                }
                            }
                            Err(reason) => {
                                self.log(&format!("remote: {from} could not open: {reason}"));
                                RemoteMessage::OpenFailed { request_id, reason }
                            }
                        }
                    }
                };
                self.send(from, &encode_remote_message(&answer)).await;
                self.publish_sessions(from).await;
            }
            RemoteMessage::Close { session_id } => {
                if self.sessions.sessions.close(&session_id) {
                    self.log(&format!("remote: {from} closed {session_id}"));
                }
                self.subscriptions.unsubscribe(from, &session_id);
                self.publish_sessions(from).await;
            }
            // Answers, not requests. A client sending one has the protocol
            // backwards; saying so is more useful than silence.
            RemoteMessage::Capabilities(_)
            | RemoteMessage::Opened { .. }
            | RemoteMessage::OpenFailed { .. }
            | RemoteMessage::Sessions { .. } => {
                self.log(&format!(
                    "remote: ignored a daemon-to-client message from {from}"
                ));
            }
        }
    }

    /// The screen half: subscribe, input, resize.
    async fn handle_screen(&mut self, from: &str, message: ScreenMessage) {
        match message {
            ScreenMessage::Subscribe {
                task_id, resync, ..
            } => {
                if self.sessions.sessions.row(&task_id).is_none() {
                    self.log(&format!(
                        "remote: {from} asked for unknown session {task_id}"
                    ));
                    return;
                }
                if let Some(previous) = self.subscriptions.subscribe(from, &task_id) {
                    self.log(&format!(
                        "remote: {from} moved from {previous} to {task_id}"
                    ));
                }
                if resync {
                    self.subscriptions.request_resync(from);
                }
            }
            ScreenMessage::Unsubscribe { task_id } => {
                self.subscriptions.unsubscribe(from, &task_id);
            }
            ScreenMessage::Input { ref task_id, .. } => {
                let task_id = task_id.clone();
                let Some(bytes) = message.input_bytes() else {
                    // A malformed payload costs that keystroke and nothing more.
                    self.log(&format!("remote: dropped malformed input from {from}"));
                    return;
                };
                // Only into a session this client is watching. Nothing else
                // establishes that a peer has any business typing here, and a
                // subscription is the one thing that says which pane it has
                // open.
                if self.subscriptions.session_for(from) != Some(task_id.as_str()) {
                    self.log(&format!(
                        "remote: refused input from {from} on {task_id} — not the session it is watching"
                    ));
                    return;
                }
                if let Err(error) = self.sessions.sessions.write(&task_id, &bytes) {
                    self.log(&format!("remote: write to {task_id} failed: {error}"));
                }
            }
            ScreenMessage::Resize {
                task_id,
                cols,
                rows,
            } => {
                if self.subscriptions.session_for(from) != Some(task_id.as_str()) {
                    return;
                }
                self.sessions.sessions.resize(&task_id, cols, rows);
            }
            // The sampler chains from the last frame it sent, so an ack is not
            // needed to decide what comes next.
            ScreenMessage::Ack { .. } => {}
            // We are the sender.
            ScreenMessage::Frame(_) => {}
            ScreenMessage::Kill { task_id, .. } => {
                self.sessions.sessions.close(&task_id);
                self.publish_sessions(from).await;
            }
        }
    }

    /// Sample every subscribed screen and send whatever changed.
    async fn publish_screens(&mut self) {
        let mut frames: Vec<(String, String)> = Vec::new();
        for (peer, session_id, stream) in self.subscriptions.iter_mut() {
            let Some(snapshot) = self.sessions.sessions.screen_rows(session_id) else {
                continue;
            };
            if let Some(frame) = stream.tick(&snapshot) {
                frames.push((
                    peer.clone(),
                    encode_screen_message(&ScreenMessage::Frame(frame)),
                ));
            }
        }
        for (peer, body) in frames {
            self.send(&peer, &body).await;
        }
    }

    /// Send `peer` the whole session list.
    pub async fn publish_sessions(&mut self, peer: &str) {
        // Refreshed here too, not only in `publish_sessions_if_changed`: this is
        // called right after a request that itself changed a row (`Open`,
        // `Close`, `Kill`, …), and without updating the cache the very next
        // tick would see that same change as "new" and send it a second time
        // to every peer.
        self.last_rows = self.current_rows();
        self.send_sessions(peer).await;
    }

    /// The session list as the client's rail draws it, freshly computed.
    fn current_rows(&mut self) -> Vec<RemoteSessionRow> {
        let now = Instant::now();
        let sample = self
            .usage_sampled_at
            .is_none_or(|last| now.duration_since(last) >= self.usage_interval);
        let snapshot = if sample {
            self.usage_sampled_at = Some(now);
            Some(medulla_monitor::sampler::ResourceSource::sample(
                &mut self.resource_source,
            ))
        } else {
            None
        };
        if sample {
            // Rebuild from this sample so exited/reaped PIDs cannot leave
            // stale process metrics attached to a still-retained session row.
            self.usage_cache.clear();
        }
        let mut rows: Vec<_> = self
            .sessions
            .sessions
            .rows()
            .iter()
            .map(|row| {
                let bracketed = self
                    .sessions
                    .sessions
                    .bracketed_paste(&row.id)
                    .unwrap_or(false);
                let mut wire = wire_row(row, bracketed);
                if row.provider != medulla::protocol::HarnessProvider::Shell {
                    if let Some(pid) = self.sessions.sessions.pid(&row.id) {
                        if let Some(process) = snapshot.as_ref().and_then(|snapshot| {
                            snapshot.processes.iter().find(|process| process.pid == pid)
                        }) {
                            let identity = (pid, process.start_time);
                            let history = self.usage_history.entry(identity).or_default();
                            history.push(process.cpu_percent);
                            if history.len() > 32 {
                                history.remove(0);
                            }
                            let usage = medulla::protocol::AgentUsage {
                                kind: sessions::agent_kind(row.provider.as_wire()),
                                pid,
                                cpu: process.cpu_percent,
                                rss: process.memory_bytes,
                                threads: process.threads,
                                started_at: process.start_time,
                                cpu_history: history.clone(),
                            };
                            self.usage_cache.insert(row.id.clone(), usage);
                        }
                    }
                }
                wire.usage = self.usage_cache.get(&row.id).cloned();
                wire
            })
            .collect();
        // Keep the process history bounded to live identities only.
        let current: std::collections::HashSet<_> = rows
            .iter()
            .filter_map(|row| {
                row.usage
                    .as_ref()
                    .map(|usage| (usage.pid, usage.started_at))
            })
            .collect();
        self.usage_history
            .retain(|identity, _| current.contains(identity));
        self.usage_cache
            .retain(|id, _| rows.iter().any(|row| &row.id == id));
        trim_usage_to_message_budget(&mut rows);
        rows
    }

    /// Send `peer` the cached session list.
    async fn send_sessions(&self, peer: &str) {
        self.send(
            peer,
            &encode_remote_message(&RemoteMessage::Sessions {
                rows: self.last_rows.clone(),
            }),
        )
        .await;
    }

    async fn send(&self, to: &str, body: &str) {
        if let Err(error) = self.bridge.send(to, body).await {
            self.log(&format!("remote: send to {to} failed: {error}"));
        }
    }
}

/// Trim only optional usage until the full reliable-channel message fits one
/// datagram. Session identity/state fields are never removed here.
fn trim_usage_to_message_budget(rows: &mut [RemoteSessionRow]) {
    let limit = medulla_link::MAX_MESSAGE_BYTES;
    while encode_remote_message(&RemoteMessage::Sessions {
        rows: rows.to_vec(),
    })
    .len()
        > limit
    {
        let oldest_history = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                row.usage
                    .as_ref()
                    .filter(|usage| !usage.cpu_history.is_empty())
                    .map(|usage| (index, usage.cpu_history.len()))
            })
            .max_by_key(|(_, history_len)| *history_len)
            .map(|(index, _)| index);
        if let Some(index) = oldest_history {
            if let Some(usage) = rows[index].usage.as_mut() {
                usage.cpu_history.remove(0);
            }
        } else if let Some(row) = rows.iter_mut().rev().find(|row| row.usage.is_some()) {
            row.usage = None;
        } else {
            // Base session rows predate this telemetry and may themselves
            // exceed the transport limit; telemetry cannot make that case
            // worse, and the existing send path reports it.
            break;
        }
    }
}
