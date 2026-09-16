//! Pure view-model for the agents a session rail lists: one lane per roster
//! agent, each carrying the tasks the hub has dispatched to it, with a row model
//! for the list and pre-wrapped transcript lines.
//!
//! Split by responsibility: `types` holds the lane/task/turn data model,
//! `lanes` seeds a lane per roster agent, `activity` fills those lanes from the
//! hub's activity log, `roster` merges the local worker registry into the roster
//! the lanes are seeded from, `rows` is the list row model, `lines` the
//! transcript rendering, `keys` the lane-key parser, and `demo` a scripted
//! lane set for the offline demo and the test suites. All public items are
//! re-exported here so callers use `medulla::ui::agents::*`.

mod activity;
mod demo;
mod keys;
mod lanes;
mod lines;
mod roster;
mod rows;
mod types;

#[cfg(test)]
mod tests;

pub use activity::merge_host_activity;
pub use demo::{
    demo_activity, demo_lanes, demo_roster, DEMO_BUSY_AGENT, DEMO_DONE_TASK, DEMO_IDLE_AGENT,
    DEMO_RUNNING_TASK,
};
pub use keys::parse_task_key;
pub use lanes::{agent_lane, roster_lanes};
pub use lines::{lane_lines, task_lines};
pub use roster::{host_descriptor, merge_host_roster};
pub use rows::{agent_row_model, agent_row_model_paged, ordered_tasks};
pub use types::{AgentLane, AgentRow, Line, TaskState, TaskStatus, TurnBlock};
