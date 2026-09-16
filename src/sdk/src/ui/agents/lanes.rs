//! Seeding one lane per roster agent.
//!
//! The lanes start empty: what each agent is doing arrives afterwards from the
//! hub's activity log through [`merge_host_activity`](super::merge_host_activity).

use crate::fleet::AgentDescriptor;

use super::types::AgentLane;

/// One lane per roster agent, in roster order, with no traffic yet.
pub fn roster_lanes(roster: &[AgentDescriptor]) -> Vec<AgentLane> {
    roster.iter().map(agent_lane).collect()
}

/// The lane for one roster agent.
///
/// The lane *key* keeps the full id — identity must stay exact, or two peers
/// sharing four leading characters would fold into one lane. Only the label is
/// shortened, and only when it is a bare address: a name someone chose is
/// meaningful text and survives whole.
pub fn agent_lane(agent: &AgentDescriptor) -> AgentLane {
    let shown = if agent.name.is_empty() {
        agent.id.as_str()
    } else {
        agent.name.as_str()
    };
    let harness_label = agent
        .metadata
        .get("harness")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|h| h.to_uppercase());
    AgentLane {
        key: format!("agent:{}", agent.id),
        label: crate::ui::util::short_if_address(shown),
        turns: Vec::new(),
        last_at: 0,
        tasks: Vec::new(),
        harness_label,
        agent_id: Some(agent.id.clone()),
        session_id: None,
        parent_agent_id: None,
        descriptor: Some(agent.clone()),
        active_tasks: 0,
        work: None,
    }
}
