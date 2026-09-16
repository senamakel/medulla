//! Chat transcript, Trace, host-link merge, and events-seam coverage:
//! the `events_changed` baseline seam, observation
//! merge into the snapshot, error/wrapped/spinner/thread-badge chat rendering,
//! and the Trace JSON detail row.

use crate::helpers::*;

// --- events_changed seam ----------------------------------------------------

#[test]
fn events_changed_flips_then_settles() {
    let (mut app, rt) = empty_app();
    // First call records the baseline (0 events) → no change reported.
    assert!(!app.events_changed());
    rt.script_event(TuiEvent::Assistant { body: "x".into() });
    app.refresh_snapshot();
    assert!(app.events_changed(), "a new event is a change");
    assert!(!app.events_changed(), "same length settles");
}

// --- host-link observation merge --------------------------------------------

#[test]
fn link_observation_merges_into_snapshot() {
    use medulla::protocol::service::LinkObservation;
    use medulla::runtime::{AgentDescriptor, AgentPresence, LinkIdentity};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let (mut app, _rt) = empty_app();
    let mut meta = serde_json::Map::new();
    meta.insert("harness".into(), serde_json::json!("link"));
    let mut presence = HashMap::new();
    presence.insert(
        "peer-1".into(),
        AgentPresence {
            online: true,
            detail: Some("idle".into()),
            at: 1,
        },
    );
    let obs = LinkObservation {
        notice: None,
        identity: Some(LinkIdentity {
            node_name: "merged-host".into(),
            forwarder: "forwarder:41641".into(),
        }),
        roster: vec![AgentDescriptor {
            id: "peer-1".into(),
            name: "peer-1".into(),
            description: "a peer".into(),
            availability: "online".into(),
            workspace_id: None,
            host_id: None,
            template_id: None,
            tags: vec![],
            metadata: meta,
        }],
        presence,
    };
    app.set_link_observation(Arc::new(Mutex::new(obs)));
    assert!(app.snapshot.link.is_some());
    assert!(app.snapshot.roster.iter().any(|a| a.id == "peer-1"));
    assert!(app.snapshot.presence.contains_key("peer-1"));
}

// --- thread badges & fork indentation ---------------------------------------

// --- Trace tab renders the JSON detail row ----------------------------------

#[test]
fn trace_tab_renders_event_and_json() {
    use medulla_tui::ui::events::NodeTrace;
    let (mut app, rt) = empty_app();
    rt.script_event(TuiEvent::Trace {
        entry: NodeTrace {
            node: "orchestrate".into(),
            ms: 42,
            tool: None,
            op: Some("decide".into()),
        },
    });
    app.refresh_snapshot();
    tab(&mut app, "Trace");
    let out = render(&mut app, 120, 40);
    assert!(out.contains("Trace ·"), "trace header");
    assert!(out.contains("orchestrate"), "trace json detail row");
}
