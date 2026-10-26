//! A harness launched through the PTY manager is registered as Medulla-owned.
//!
//! The Resources tab tells Medulla's own harnesses from external ones by the
//! launch registry in the SDK. Nothing but tests used to fill it, so every
//! harness Medulla started showed up as external. A real `/bin/sh` on a real
//! pty stands in for the harness: ownership is keyed by pid and start time, which
//! only a live process has.
//!
//! Unix-only: it runs `/bin/sh` on a pty, which Windows has no equivalent of.

#![cfg(unix)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use medulla::protocol::HarnessProvider;
use medulla::resources::launch::agent_registry;
use medulla_tui::worker::pty::{LaunchSpec, PtyManager, SessionControl, SessionOrigin};

/// Open a session whose child simply stays alive.
fn idle_session(sessions: &PtyManager) -> String {
    let mut env = HashMap::new();
    if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".to_string(), path);
    }
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    sessions
        .open(LaunchSpec {
            provider: HarnessProvider::Codex,
            preset: None,
            bin: "/bin/sh".to_string(),
            cwd: "/".to_string(),
            env,
            extra_args: vec!["-c".to_string(), "sleep 30".to_string()],
            skip_permissions: false,
            label: "you:codex".to_string(),
            model: None,
            session_id: None,
            control: SessionControl::User,
            origin: SessionOrigin::User,
            name: None,
            mcp_grant_session: None,
        })
        .expect("open a session")
}

/// Spin until `check` passes; children on real ptys are at the mercy of load.
fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn a_launched_harness_is_registered_until_it_exits() {
    let sessions = PtyManager::new();
    let id = idle_session(&sessions);
    let pid = sessions.pid(&id).expect("the child has a pid");
    let registered = || {
        agent_registry().snapshot().iter().any(|entry| {
            // Keyed by the manager's id so Enter on the Agents row finds the
            // session in the Sessions rail.
            entry.pid == pid
                && entry.provider == "codex"
                && entry.session_id.as_deref() == Some(id.as_str())
        })
    };
    assert!(registered(), "a launched harness must be Medulla-owned");

    sessions.close(&id);
    wait_for("the registration to be released", || !registered());
    sessions.shutdown();
}
