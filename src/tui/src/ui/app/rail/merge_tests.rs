//! Folding a dispatch this device serves into the row that is already on the
//! rail for it.
//!
//! A served dispatch reaches the rail from both surfaces at once — the hub
//! knows the task, and the pty manager knows the local session running it — so
//! something has to decide they are one row. These cover which row the live pty
//! is folded into, without standing up a hub to answer the task-to-session
//! lookup for real.

use super::tests::{app, stub_session};
use super::{task_row_serving, AgentGroup, AgentRailRow, SessionRailRow};
use medulla::ui::agents::{AgentLane, AgentRole, TaskState, TaskStatus};

fn task_row(task_id: &str) -> SessionRailRow {
    SessionRailRow {
        agent_id: Some("shell".to_string()),
        lane_index: Some(0),
        task: Some(TaskState {
            task_id: task_id.to_string(),
            status: TaskStatus::Running,
            turns: 0,
            last_at: 0,
            turn_blocks: Vec::new(),
            attention: None,
            question_id: None,
            work: None,
        }),
        local: None,
        last: false,
    }
}

fn group(sessions: Vec<SessionRailRow>) -> AgentGroup {
    AgentGroup {
        row: AgentRailRow {
            agent_id: "shell".to_string(),
            host_id: String::new(),
            agent: None,
            lane_index: Some(0),
        },
        sessions,
        visible_tasks: 0,
        hidden: 0,
        overflow: false,
    }
}

#[test]
fn the_task_row_takes_the_session_it_is_being_served_by() {
    let mut owner = group(vec![task_row("t-other"), task_row("t-1")]);
    let mut groups = vec![&mut owner];

    let found = task_row_serving(&mut groups, "w_1", |task_id| match task_id {
        "t-1" => Some("w_1".to_string()),
        _ => Some("w_9".to_string()),
    })
    .expect("the task this pty is serving");

    assert_eq!(
        found.task.as_ref().map(|task| task.task_id.as_str()),
        Some("t-1"),
        "the merge must pick the task the daemon says this pty serves, \
         not the first task row on the rail"
    );
}

#[test]
fn a_task_row_that_already_has_a_session_is_left_alone() {
    // Otherwise a second live pty would collapse onto a task already being
    // served, and the rail would lose a running harness.
    let mut taken = task_row("t-1");
    taken.local = Some(stub_session("w_1"));
    let mut owner = group(vec![taken]);
    let mut groups = vec![&mut owner];

    assert!(
        task_row_serving(&mut groups, "w_2", |_| Some("w_2".to_string())).is_none(),
        "a row already carrying a session is not a merge target"
    );
}

#[test]
fn a_settled_task_keeps_its_retained_session_on_a_row_of_its_own() {
    // `session_for_task` goes through the daemon's *running* map, so it
    // answers `None` the moment the task settles — and a retained session
    // needs its own row to put the cursor on.
    let mut owner = group(vec![task_row("t-1")]);
    let mut groups = vec![&mut owner];

    assert!(task_row_serving(&mut groups, "w_1", |_| None).is_none());
}

#[test]
fn paging_starts_with_the_fold_running_first_task_order() {
    let lane = AgentLane {
        key: "agent:shell".into(),
        label: "shell".into(),
        role: AgentRole::Agent,
        turns: Vec::new(),
        last_at: 0,
        tasks: vec![
            TaskState {
                task_id: "completed-recently".into(),
                status: TaskStatus::Done,
                turns: 0,
                last_at: 900,
                turn_blocks: Vec::new(),
                attention: None,
                question_id: None,
                work: None,
            },
            TaskState {
                task_id: "running-older".into(),
                status: TaskStatus::Running,
                turns: 0,
                last_at: 100,
                turn_blocks: Vec::new(),
                attention: None,
                question_id: None,
                work: None,
            },
        ],
        context_tokens: None,
        usage: Default::default(),
        harness_label: None,
        agent_id: Some("shell".into()),
        session_id: None,
        parent_agent_id: None,
        descriptor: None,
        active_tasks: 1,
        work: None,
    };

    let group = app().group_for_lane(&lane, 0);

    assert_eq!(
        group
            .sessions
            .iter()
            .filter_map(|session| session.task.as_ref().map(|task| task.task_id.as_str()))
            .collect::<Vec<_>>(),
        vec!["running-older", "completed-recently"],
        "the first task page must match the fold's running-first order"
    );
}
