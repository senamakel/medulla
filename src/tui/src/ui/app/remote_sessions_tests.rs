//! Tests for id resolution and the routed session lookups.
//!
//! The property under test throughout is that a *local* id behaves exactly as it
//! always has. Every routed accessor added here is on a path the local case
//! already used, so a mistake in the parsing would not fail loudly — it would
//! quietly send a local keystroke nowhere.

use medulla::protocol::{RemoteAttention, RemoteSessionRow, RemoteSessionState};

use super::remote_sessions::{qualify, session_row, SessionRef};

fn remote_row(id: &str) -> RemoteSessionRow {
    RemoteSessionRow {
        id: id.to_string(),
        label: "you:shell:zsh".to_string(),
        provider: "shell".to_string(),
        preset: None,
        state: RemoteSessionState::Running,
        cwd: "/work".to_string(),
        name: Some("build".to_string()),
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_001_000,
        busy: false,
        working: true,
        attention: None,
        bracketed_paste: true,
        last_error: None,
    }
}

#[test]
fn a_bare_id_is_local() {
    // The rule every existing caller depends on. A pty id has no separator in
    // it, so nothing that worked before can start resolving as remote.
    assert_eq!(SessionRef::parse("w_7"), SessionRef::Local("w_7"));
    assert_eq!(SessionRef::parse(""), SessionRef::Local(""));
}

#[test]
fn a_prefixed_id_names_its_host_and_session() {
    assert_eq!(
        SessionRef::parse("tower/w_7"),
        SessionRef::Remote {
            host: "tower",
            session: "w_7"
        }
    );
}

#[test]
fn a_malformed_prefix_falls_back_to_local_rather_than_half_resolving() {
    // Half a remote id is not a remote session. Treating it as one would send a
    // keystroke to a host named "" — better to fail the local lookup, which says
    // so.
    assert_eq!(SessionRef::parse("/w_7"), SessionRef::Local("/w_7"));
    assert_eq!(SessionRef::parse("tower/"), SessionRef::Local("tower/"));
}

#[test]
fn qualify_and_parse_are_inverses() {
    let id = qualify("tower", "w_7");
    assert_eq!(id, "tower/w_7");
    assert_eq!(
        SessionRef::parse(&id),
        SessionRef::Remote {
            host: "tower",
            session: "w_7"
        }
    );
}

#[test]
fn a_remote_row_becomes_a_session_row_the_rail_can_place() {
    let row = session_row("tower", &remote_row("w_1"));
    assert_eq!(row.id, "tower/w_1");
    assert_eq!(row.cwd, "/work");
    assert_eq!(row.name.as_deref(), Some("build"));
    assert!(row.working);
    assert_eq!(row.provider, medulla::protocol::HarnessProvider::Shell);
    assert!(row.state.is_running());
}

#[test]
fn a_remote_session_is_always_the_operators() {
    // Nothing dispatches into a remote session — it exists because somebody
    // asked for it in the picker. The orchestrator must never see one as
    // available, so both fields are fixed rather than carried.
    let row = session_row("tower", &remote_row("w_1"));
    assert_eq!(row.control, crate::worker::pty::SessionControl::User);
    assert!(row.origin.is_user());
}

#[test]
fn a_remote_row_claims_no_checkout_it_cannot_see() {
    // The Changes view reads these. A fabricated checkout would have it claim to
    // know a repository on a filesystem this machine cannot reach.
    let row = session_row("tower", &remote_row("w_1"));
    assert!(row.launch_root.is_none());
    assert!(row.launch_commit.is_none());
    assert!(row.mcp_grant_session.is_none());
    assert_eq!(row.checkout, medulla::ui::checkout::Checkout::default());
}

#[test]
fn an_exited_remote_session_keeps_its_code() {
    let mut remote = remote_row("w_1");
    remote.state = RemoteSessionState::Exited { code: Some(130) };
    let row = session_row("tower", &remote);
    assert!(matches!(
        row.state,
        crate::worker::pty::PtyState::Exited { code: Some(130) }
    ));
}

#[test]
fn a_named_attention_kind_survives_the_wire() {
    let mut remote = remote_row("w_1");
    remote.attention = Some(RemoteAttention {
        summary: "allow this command?".to_string(),
        kind: "approval".to_string(),
    });
    let row = session_row("tower", &remote);
    let attention = row.attention.expect("the cue should cross");
    assert_eq!(attention.kind, crate::worker::pty::AttentionKind::Approval);
    assert_eq!(attention.what, "allow this command?");
}

#[test]
fn an_unknown_attention_kind_still_raises_a_cue() {
    // A newer peer naming a kind this build has never heard of is still a
    // harness waiting on somebody. Dropping the cue would hide a stuck session,
    // which is the one thing the rail exists to surface.
    let mut remote = remote_row("w_1");
    remote.attention = Some(RemoteAttention {
        summary: "something new".to_string(),
        kind: "teleport-consent".to_string(),
    });
    let row = session_row("tower", &remote);
    assert!(
        row.attention.is_some(),
        "an unrecognized kind must not silence the cue"
    );
}
