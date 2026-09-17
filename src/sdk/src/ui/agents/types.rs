//! Data model for the agent lanes: the lane/task/turn structs, the status enum
//! with its trivial classification impls, the pre-styled display [`Line`], and
//! the [`AgentRow`] list-row enum. All behaviour lives in the sibling logic
//! modules; this file holds only the shapes and their trivial accessors.

use crate::fleet::AgentDescriptor;

/// Terminal-or-running status of a delegated task lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// The task is still running.
    Running,
    /// The task completed successfully.
    Done,
    /// The task failed.
    Failed,
    /// §3.3(3): the wire keeps `cancelled` distinct from `failed`; the fold does too.
    Cancelled,
}

impl TaskStatus {
    /// Map a `task_complete` wire status onto the lane status. Unknown → Running is
    /// never produced here (a completion is terminal); an unrecognized string falls
    /// back to `failed` rather than silently reading as done.
    pub fn from_wire(s: &str) -> TaskStatus {
        match s {
            "done" => TaskStatus::Done,
            "cancelled" => TaskStatus::Cancelled,
            _ => TaskStatus::Failed,
        }
    }
    /// The lowercase wire label for this status.
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::Running => "running",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }
    /// The display colour for this status.
    pub fn color(self) -> &'static str {
        match self {
            TaskStatus::Running => "yellow",
            TaskStatus::Done => "green",
            TaskStatus::Failed => "red",
            TaskStatus::Cancelled => "gray",
        }
    }
}

/// One folded turn in a lane or task transcript.
#[derive(Debug, Clone, Default)]
pub struct TurnBlock {
    /// Wire timestamp (ms) of the turn.
    pub at: i64,
    /// The turn header line.
    pub header: String,
    /// Optional colour override for the header.
    pub header_color: Option<String>,
    /// Optional reasoning/thinking body.
    pub reasoning: Option<String>,
    /// Optional output content body.
    pub content: Option<String>,
    /// Rendered tool-call lines.
    pub tools: Vec<String>,
}

/// The per-task state accumulated for an agent-identity lane.
#[derive(Debug, Clone)]
pub struct TaskState {
    /// The wire task id.
    pub task_id: String,
    /// Current status.
    pub status: TaskStatus,
    /// Number of folded turns.
    pub turns: usize,
    /// Timestamp (ms) of the last activity.
    pub last_at: i64,
    /// The folded turn blocks for this task.
    pub turn_blocks: Vec<TurnBlock>,
    /// A pending `task_attention` prompt (reason · content), cleared on completion.
    pub attention: Option<String>,
    /// The `questionId` of a pending `task_attention` — the handle `question.answer`
    /// needs. `None` when the task has no open question.
    pub question_id: Option<String>,
    /// What the worker running this task says it is working on: its todo list,
    /// plan, sub-agents, and file edits, as of the last frame it sent.
    ///
    /// `None` when the worker reported nothing structured — an older peer, or a
    /// harness with no such surface — which renders as no work panel at all
    /// rather than an empty one.
    ///
    /// Boxed because a [`TaskState`] is carried by value inside every
    /// [`AgentRow`] the list model builds each frame, and a snapshot is far
    /// larger than the rest of the row put together.
    pub work: Option<Box<crate::harness_work::WorkSnapshot>>,
}

/// One lane in the Sessions view: a roster agent and the tasks dispatched to it.
#[derive(Debug, Clone)]
pub struct AgentLane {
    /// The lane-unique key.
    pub key: String,
    /// The display label.
    pub label: String,
    /// The lane-level transcript turns.
    pub turns: Vec<TurnBlock>,
    /// Timestamp (ms) of the last activity.
    pub last_at: i64,
    /// Per-task state for agent-identity lanes.
    pub tasks: Vec<TaskState>,
    /// A harness label tag, if learned.
    pub harness_label: Option<String>,
    /// The backing agent id, if any.
    pub agent_id: Option<String>,
    /// The backing session id, for session lanes.
    pub session_id: Option<String>,
    /// The parent agent id, for session lanes grouped under a machine.
    pub parent_agent_id: Option<String>,
    /// The roster descriptor, for roster-seeded lanes.
    pub descriptor: Option<AgentDescriptor>,
    /// Count of currently-active tasks.
    pub active_tasks: i64,
    /// The most recent work snapshot across this lane's tasks, so a lane header
    /// can show what the machine is doing without opening a task.
    pub work: Option<Box<crate::harness_work::WorkSnapshot>>,
}

/// A single pre-styled display row.
#[derive(Debug, Clone, Default)]
pub struct Line {
    /// The row text.
    pub text: String,
    /// Optional colour.
    pub color: Option<String>,
    /// Whether the row renders dimmed.
    pub dim: bool,
}

/// One printed row of the agent list.
#[derive(Debug, Clone)]
pub enum AgentRow {
    /// A lane header row.
    Lane {
        /// Index into the lanes slice.
        lane_index: usize,
    },
    /// A per-task sublane row under an agent-identity lane.
    Sub {
        /// Index into the lanes slice.
        lane_index: usize,
        /// The task rendered by this sublane.
        task: TaskState,
        /// Whether this is the last shown sublane.
        last: bool,
    },
    /// The overflow row under a lane whose sublanes are paged.
    ///
    /// Reads `+N more` while sublanes are still hidden, and `show less` once the
    /// lane is fully revealed — one row for both directions, so paging a lane
    /// open and folding it shut are the same keystroke in the same place.
    More {
        /// Index into the lanes slice.
        lane_index: usize,
        /// Number of hidden sublanes; zero when this row only offers to collapse.
        hidden: usize,
    },
}

impl AgentRow {
    /// The lane index this row belongs to, if any.
    pub fn lane_index(&self) -> Option<usize> {
        match self {
            AgentRow::Lane { lane_index }
            | AgentRow::Sub { lane_index, .. }
            | AgentRow::More { lane_index, .. } => Some(*lane_index),
        }
    }
    /// Whether this row is selectable in the list.
    ///
    /// Every row is: the overflow row is the control that pages a lane's
    /// sublanes open and shut, so the cursor has to be able to reach it.
    pub fn selectable(&self) -> bool {
        true
    }
}
