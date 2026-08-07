//! Cursor identity tests for the Agents rail.
//!
//! The live rail is rebuilt on every frame, so these cover the stable-anchor
//! resolver independently of the `App` rendering loop.

use super::{resolve_rail_cursor, AgentRailRow, RailAnchor, RailRow};

fn agent(id: &str) -> RailRow {
    RailRow::Agent(AgentRailRow {
        agent_id: id.to_string(),
        host_id: String::new(),
        agent: None,
        lane_index: None,
    })
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
