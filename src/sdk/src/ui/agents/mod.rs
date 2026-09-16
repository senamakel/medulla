//! Pure view-model for the agents a session rail lists: one lane per roster
//! agent, each carrying the tasks the hub has dispatched to it, with a row model
//! for the list and pre-wrapped transcript lines.
//!
//! Split by responsibility: `types` holds the lane/task/turn data model,
//! `lanes` seeds a lane per roster agent, `activity` fills those lanes from the
//! hub's activity log, `roster` merges the local worker registry into the roster
//! the lanes are seeded from, `rows` is the list row model, `lines` the
//! transcript rendering, and `keys` the lane-key parser. All public items are
//! re-exported here so callers use `medulla::ui::agents::*`.

mod activity;
mod keys;
mod lanes;
mod lines;
mod roster;
mod rows;
mod types;

#[cfg(test)]
mod tests;

pub use activity::merge_host_activity;
pub use keys::parse_task_key;
pub use lanes::{agent_lane, roster_lanes};
pub use lines::{lane_lines, task_lines};
pub use roster::{host_descriptor, merge_host_roster};
pub use rows::{agent_row_model, agent_row_model_paged, ordered_tasks};
pub use types::{AgentLane, AgentRow, Line, TaskState, TaskStatus, TurnBlock};
