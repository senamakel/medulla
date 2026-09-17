//! A scripted lane set for the offline demo and the test suites.
//!
//! Two agents with the shape a real hub produces: one busy on a task with a
//! work snapshot behind it, one that has finished. Built through the same
//! [`roster_lanes`] + [`merge_host_activity`] path the app uses, so a
//! fixture cannot drift from what the hub would have produced.

use serde_json::json;

use crate::fleet::AgentDescriptor;
use crate::harness_work::{kinds, WorkFold};
use crate::hub::WorkerActivity;

use super::{merge_host_activity, roster_lanes, AgentLane};

/// The roster id of the demo agent that is mid-task.
pub const DEMO_BUSY_AGENT: &str = "dev-1";
/// The task [`DEMO_BUSY_AGENT`] is running.
pub const DEMO_RUNNING_TASK: &str = "task-1";
/// The roster id of the demo agent whose task has finished.
pub const DEMO_IDLE_AGENT: &str = "dev-2";
/// The task [`DEMO_IDLE_AGENT`] finished.
pub const DEMO_DONE_TASK: &str = "task-2";

/// The demo roster: two remote coding agents on one host.
pub fn demo_roster() -> Vec<AgentDescriptor> {
    let agent = |id: &str, description: &str| AgentDescriptor {
        id: id.into(),
        name: id.into(),
        description: description.into(),
        availability: "online".into(),
        workspace_id: Some("ws-1".into()),
        host_id: None,
        template_id: Some("implementer".into()),
        tags: vec!["code".into()],
        metadata: [
            ("harness".to_string(), json!("codex")),
            ("workspace".to_string(), json!("/srv/repos/auth")),
        ]
        .into_iter()
        .collect(),
    };
    vec![
        agent(
            DEMO_BUSY_AGENT,
            "A remote coding agent for delegated implementation work.",
        ),
        agent(DEMO_IDLE_AGENT, "A second agent on the same checkout."),
    ]
}

/// The activity the hub would have observed for the demo roster: one task
/// still running, one settled with a reply.
pub fn demo_activity() -> Vec<WorkerActivity> {
    let at = 1_700_000_000_000;
    let mut work = WorkFold::new();
    for (kind, payload) in demo_work_events() {
        work.apply(kind, &payload, at);
    }
    let record = |agent: &str, task: &str, kind: &str, content: &str, at: i64| WorkerActivity {
        agent_id: agent.into(),
        task_id: task.into(),
        kind: kind.into(),
        content: content.into(),
        at,
        work: None,
    };
    vec![
        record(DEMO_BUSY_AGENT, DEMO_RUNNING_TASK, "ack", "accepted", at),
        WorkerActivity {
            work: Some(Box::new(work.into_snapshot())),
            ..record(
                DEMO_BUSY_AGENT,
                DEMO_RUNNING_TASK,
                "status",
                "Reading auth module…",
                at + 1_000,
            )
        },
        record(
            DEMO_IDLE_AGENT,
            DEMO_DONE_TASK,
            "ack",
            "accepted",
            at + 2_000,
        ),
        record(
            DEMO_IDLE_AGENT,
            DEMO_DONE_TASK,
            "reply",
            "Refactored auth into 3 focused modules.",
            at + 60_000,
        ),
    ]
}

/// The demo lanes: [`demo_roster`] with [`demo_activity`] folded in.
pub fn demo_lanes() -> Vec<AgentLane> {
    let mut lanes = roster_lanes(&demo_roster());
    merge_host_activity(&mut lanes, &demo_activity());
    lanes
}

/// The worker's own screen for the running demo task: what it plans to do,
/// what it has ticked off, who it delegated to, and what it rewrote.
fn demo_work_events() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            kinds::SESSION_INFO,
            json!({ "model": "gpt-5-codex", "cwd": "/srv/repos/auth", "tools": ["shell", "apply_patch", "web_search"] }),
        ),
        (
            kinds::PLAN_UPDATE,
            json!({ "goal": "Split auth into focused modules" }),
        ),
        (
            kinds::TODO_UPDATE,
            json!({ "todos": [
                { "content": "map the auth surface", "status": "completed" },
                { "content": "extract the token store", "status": "completed" },
                { "content": "split the session guard", "status": "in_progress", "activeForm": "Splitting the session guard" },
                { "content": "update the callers", "status": "pending" },
            ]}),
        ),
        (
            kinds::SUBAGENT_START,
            json!({ "call_id": "sub-1", "agent_type": "code-reviewer", "description": "review the extracted token store" }),
        ),
        (
            kinds::FILE_CHANGE,
            json!({ "path": "src/auth/tokens.rs", "change": "added" }),
        ),
        (
            kinds::FILE_CHANGE,
            json!({ "path": "src/auth/mod.rs", "change": "modified" }),
        ),
    ]
}
