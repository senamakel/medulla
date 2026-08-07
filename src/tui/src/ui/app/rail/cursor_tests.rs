//! Cursor identity tests for the Agents rail.
//!
//! The live rail is rebuilt on every frame, so these cover the stable-anchor
//! resolver independently of the `App` rendering loop.

use crate::ui::agents::{AgentLane, AgentRole, AgentRow};

use super::{rail_anchor, resolve_rail_cursor, AgentRailRow, RailAnchor, RailRow};

fn agent(id: &str) -> RailRow {
    RailRow::Agent(AgentRailRow {
        agent_id: id.to_string(),
        host_id: String::new(),
        agent: None,
        lane_index: None,
    })
}

fn lane(key: &str) -> AgentLane {
    AgentLane {
        key: key.to_string(),
        label: String::new(),
        role: AgentRole::Agent,
        turns: Vec::new(),
        last_at: 0,
        tasks: Vec::new(),
        context_tokens: None,
        usage: Default::default(),
        harness_label: None,
        agent_id: None,
        session_id: None,
        parent_agent_id: None,
        descriptor: None,
        active_tasks: 0,
        work: None,
    }
}

#[test]
fn an_anchored_agent_follows_rows_inserted_ahead_of_it() {
    let anchor = RailAnchor::Agent("builder".to_string());
    let rows = vec![RailRow::NewAgent, agent("scout"), agent("builder")];

    assert_eq!(resolve_rail_cursor(&rows, &[], Some(&anchor), 0), 2);
}

#[test]
fn a_missing_anchor_uses_the_clamped_previous_offset() {
    let rows = vec![RailRow::NewAgent, agent("builder")];
    let anchor = RailAnchor::Agent("removed".to_string());

    assert_eq!(resolve_rail_cursor(&rows, &[], Some(&anchor), 99), 1);
}

#[test]
fn an_overflow_anchor_uses_its_lanes_stable_key() {
    let lanes = vec![lane("builder")];
    let overflow = RailRow::Lane(AgentRow::More {
        lane_index: 0,
        hidden: 3,
    });

    let anchor = rail_anchor(&overflow, &lanes);
    assert_eq!(anchor, Some(RailAnchor::Overflow("builder".to_string())));

    let rows = vec![RailRow::NewAgent, agent("new"), overflow];
    assert_eq!(resolve_rail_cursor(&rows, &lanes, anchor.as_ref(), 0), 2);
}
