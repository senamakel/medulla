//! The data model for the interactive TUI screen: the tab list, multi-pane
//! navigation constants, the [`Cmd`] the event loop runs on the app's behalf, the
//! small overlay/state types ([`ResumePicker`], [`Prompt`], [`PromptKind`],
//! and the central [`App`] struct itself.
//!
//! Behaviour lives in the sibling modules ([`super::super::state`], [`super::super::input`],
//! [`super::super::keys`], [`super::super::commands`], and [`super::super::render`]), each of which
//! adds its own `impl App` block. Because those blocks share `App`'s private
//! fields, the fields (and the private helper types/consts here) are
//! `pub(in crate::ui::app)` so every sibling submodule can reach them.

use std::sync::Arc;

use ratatui::layout::Rect;

use crate::ui::composer::Draft;
use crate::ui::theme::Theme;
use medulla::client::{FeedbackComment, FeedbackItem, FeedbackQuery, FeedbackType};
use medulla::config::LoadedConfig;
use medulla::runtime::{ContextItem, Runtime, RuntimeSnapshot, WorkerOp};

use super::picker::*;
use super::rail_hit::RailHit;

include!("settings_nav.inc.rs");

/// What the pane beside the rail is showing for the selected harness.
///
/// The harness screen is not the only thing worth looking at for a session, and
/// the alternatives are all *about* that session rather than beside it: what it
/// has changed, and — as more of them land — what it is running. So they take
/// the pane's real estate rather than opening somewhere else, the way a tab
/// switch replaces a page: one thing on screen, one key to swap it, and the
/// rail cursor never moves.
///
/// Scoped to the selected session and reset when the cursor moves off it
/// ([`App::resolve_selected_session`](crate::ui::app)): a view opened to answer
/// a question about one harness must not stay open over the next one, where it
/// would be showing another session's diff under this session's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneView {
    /// The harness's own terminal — what it is painting right now.
    #[default]
    Harness,
    /// What the harness has changed since it launched.
    Diff,
}

/// Which pane of the Workflows tab has the keyboard.
///
/// Focus is split by *mode*, the way Settings and Routing split theirs: the
/// sidebar picks what is being looked at and hands over with `Enter`, the canvas
/// walks the graph, and the copilot is a composer that takes every printable
/// key. `Esc` steps back out one level at a time.
///
/// `Tab` is not part of this. It belongs to the top-level view ring, and a tab
/// that cycled its own panes with it would be a tab inside a tab — `c` reaches
/// the copilot instead.
#[cfg(feature = "workflows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowFocus {
    /// The catalogue sidebar: arrows walk workflows and their runs.
    #[default]
    Sidebar,
    /// The graph canvas: arrows walk nodes along their edges.
    Canvas,
    /// The copilot composer: printable keys type, Enter sends.
    Copilot,
}

/// What the Workflows content pane is showing.
///
/// One view at a time, beside the catalogue sidebar — the same two-pane shape
/// as Routing and Settings. Derived from [`WorkflowFocus`] and the inspector
/// toggle by [`App::workflow_view`] rather than stored, so it cannot drift from
/// the state that decides it.
#[cfg(feature = "workflows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowView {
    /// The laid-out graph, with any selected run overlaid on it.
    Graph,
    /// The selected node's declaration, and how a run left it.
    Inspector,
    /// The conversation that edits the graph.
    Copilot,
}

/// Everything the Workflows tab holds that is not the catalogue itself.
///
/// Grouped into one struct rather than a dozen `workflow_*` fields on [`App`]:
/// the tab has three panes with their own cursors, and a flat namespace made it
/// impossible to see which cursor belonged to which pane.
#[cfg(feature = "workflows")]
#[derive(Debug, Default)]
pub struct WorkflowsState {
    /// Which pane has the keyboard.
    pub(in crate::ui::app) focus: WorkflowFocus,
    /// Whether the rail cursor is on the "New workflow" row.
    ///
    /// Its own flag rather than a sentinel value of the catalogue index,
    /// because the New row is not a workflow: it has no graph to draw, no runs
    /// to list, and nothing to run. Everything that reads the selection has to
    /// answer "or is it the new one?" and a magic index would let that question
    /// go unasked.
    pub(in crate::ui::app) creating: bool,
    /// The selected workflow's run, when the rail cursor is on one of the run
    /// rows nested under it rather than on the workflow itself.
    ///
    /// A tree cursor rather than an index into a flattened row list: the rows
    /// under a workflow only exist while it is selected, so a flat index would
    /// have to be reinterpreted every time the cursor crossed a workflow
    /// boundary — and gets it wrong the moment a run appears mid-scroll.
    pub(in crate::ui::app) run_index: Option<usize>,
    /// The selected workflow's graph, as last read from the store. Cached
    /// because a render pass must not touch the disk, and re-laying it out every
    /// frame would move boxes under the cursor.
    pub(in crate::ui::app) graph: Option<Box<medulla::workflows::WorkflowGraph>>,
    /// The selected workflow's own choice of harness and model, cached with the
    /// graph and for the same reason. Not part of the graph, so a preview
    /// reading only [`graph`](Self::graph) would report the host's harness for a
    /// workflow that pinned its own.
    pub(in crate::ui::app) defaults: medulla::workflows::WorkflowDefaults,
    /// The laid-out form of [`graph`](Self::graph).
    pub(in crate::ui::app) layout: medulla::ui::workflows::GraphLayout,
    /// Selected node in the canvas, in the layout's reading order.
    pub(in crate::ui::app) node_index: usize,
    /// Which outline row [`node_index`](Self::node_index) sits on, when the
    /// cursor last moved by outline navigation.
    ///
    /// A join is drawn on two rows — the subtree and its `revisit` reference —
    /// that share one `node_index`, so the index alone cannot tell which row is
    /// current. `outline_cursor_row` trusts this row only while it still points
    /// at `node_index`; any selection that changes `node_index` without going
    /// through [`App::move_outline_cursor`] (the rail, the canvas, a fresh
    /// workflow) leaves it stale and falls back to resolving by node.
    pub(in crate::ui::app) outline_row: usize,
    /// Vertical scroll of the canvas, in rows.
    ///
    /// The only scroll the canvas has: the graph folds onto a new band whenever
    /// a layer would run past the right edge, so it is never wider than the
    /// pane and there is nothing to scroll horizontally. Counted in rows rather
    /// than lanes because a fold puts a band boundary between two lanes, and a
    /// scroll measured in lanes cannot address the gap.
    pub(in crate::ui::app) canvas_row: usize,
    /// Rows inside the graph panel during its most recent render.
    ///
    /// Navigation uses this measured viewport rather than the full terminal
    /// height, because the selected-node preview shares the content column.
    pub(in crate::ui::app) graph_rows: usize,
    /// Top line of the rich selected-step preview.
    pub(in crate::ui::app) preview_scroll: usize,
    /// Whether the graph is drawn as an outline, when the operator has said.
    ///
    /// `None` — the default — leaves the choice to the graph itself (see
    /// `App::prefers_outline`); `v` pins it either way for this session. Stored
    /// per tab rather than per workflow because it is a reading preference, and
    /// an operator who wants wires wants them for the next workflow too.
    pub(in crate::ui::app) outline: Option<bool>,
    /// Whether the inspector below the canvas is expanded over it.
    pub(in crate::ui::app) inspector_open: bool,
    /// The run being overlaid on the graph, when a run row is selected.
    pub(in crate::ui::app) overlay: Option<medulla::workflows::RunId>,
    /// The Sessions-rail run this state was last pointed at, so the mirror is
    /// re-established only when the rail cursor actually moves.
    ///
    /// The Sessions tab draws the workflow canvas inline for a selected run, which
    /// means every frame would otherwise call
    /// [`select_workflow`](crate::ui::app::App::select_workflow) — and that
    /// re-reads the run store and re-lays out the graph, both off the disk, at
    /// the app's full draw rate. Remembering the id turns that into one read per
    /// cursor move. `None` while the Agents cursor is not on a run, so stepping
    /// off a run and back onto it re-syncs rather than trusting a stale graph.
    pub(in crate::ui::app) mirrored_run: Option<String>,
    /// The newest report included in [`mirrored_run`](Self::mirrored_run).
    ///
    /// A live run keeps its identity while its current graph node and durable
    /// record change. Remembering the report generation lets the mirror avoid
    /// disk work on ordinary redraws without freezing the canvas at its first
    /// observed node.
    pub(in crate::ui::app) mirrored_run_updated_at: Option<i64>,
    /// One copilot thread per workflow, so switching in the rail does not show
    /// the previous workflow's conversation or lose this one's.
    pub(in crate::ui::app) copilots:
        std::collections::HashMap<String, medulla::ui::workflows::CopilotState>,
    /// The copilot composer's draft.
    pub(in crate::ui::app) draft: Draft,
    /// Scroll offset in the copilot transcript, in lines from the bottom.
    pub(in crate::ui::app) copilot_scroll: usize,
}

/// An async action the event loop must run on the app's behalf.
#[derive(Debug)]
pub enum Cmd {
    /// Exit the application.
    Quit,
    /// Submit a composer line as a new conversational turn.
    Submit(String),
    /// Resume a previously saved chat by session id.
    Resume(String),
    /// Fetch the list of resumable chats for the resume picker.
    ListChats,
    /// Bootstrap a `[[remoteHosts]]` machine and learn what it can do.
    ///
    /// Runs `ssh`, so it must never be on the render path. Issued the first time
    /// an operator picks the host, not at startup — a configured machine that is
    /// switched off should cost nothing until somebody asks for it.
    ConnectRemoteHost {
        /// The `[[remoteHosts]]` id to dial.
        host_id: String,
    },
    /// Re-inspect the runtime's context chunks for the Context tab.
    InspectContext,
    /// Clear the session this host is signed in with.
    Logout,
    /// Apply a worker fleet mutation.
    WorkerOp(WorkerOp),
    /// Apply several fleet mutations as one operator action.
    ///
    /// Removing a *host* is the case this exists for: a host is a group of
    /// roster entries sharing an address, and the registry has no host-level
    /// op — so taking one out means taking each of its agents out. Carrying
    /// them together keeps that one keypress one status line, rather than N
    /// racing "Worker registry updated" messages for what the operator did
    /// once. They are applied in order, and a failure reports the op it
    /// stopped on instead of being swallowed by the next success.
    WorkerOps(Vec<WorkerOp>),
    /// Retarget the live screen subscription: stop watching one task, start
    /// watching another. Both halves ride one command so the change is atomic
    /// from the loop's point of view — a stop that landed without its start
    /// would leave the pane blank with nothing on the way.
    WatchTask {
        /// The `(worker address, task id)` to stop streaming, if any.
        stop: Option<(String, String)>,
        /// The `(worker address, task id)` to start streaming, if any.
        start: Option<(String, String)>,
    },
    /// Kill the session serving a watched task after UI confirmation.
    KillTask {
        /// The worker address that owns the session.
        worker: String,
        /// The dispatched task whose session should be killed.
        task_id: String,
    },
    /// Load embedded compression totals from the configured workspace.
    LoadCompressionStats { workspace: std::path::PathBuf },
    /// Fetch account-level usage from the backend for the Usage tab.
    LoadUsage,
    /// Re-sample the subscription-usage meters that are switched on.
    ///
    /// Carries the account-usage document the Usage page loaded, since the
    /// portal meter reads its balance from that rather than issuing a second
    /// request; `None` when it has not been fetched yet.
    LoadSubscriptions(Option<serde_json::Value>),
    /// Collect local provider and customization facts for the Harnesses page.
    LoadHarnesses {
        /// Effective configuration files, ordered from lowest to highest precedence.
        sources: Vec<String>,
        /// Medulla's per-account root for managed skills, when supplied at startup.
        medulla_home: Option<std::path::PathBuf>,
    },
    /// Load a page of the feedback board for the Feedback surface.
    LoadFeedback(FeedbackQuery),
    /// Load one board item's comments for the detail pane.
    LoadFeedbackDetail(String),
    /// Cast, change, or retract a vote on a board item.
    VoteFeedback {
        /// The item being voted on.
        id: String,
        /// `1` upvote, `-1` downvote, `0` retract.
        value: i8,
    },
    /// Post a comment on a board item.
    CommentFeedback {
        /// The item being commented on.
        id: String,
        /// The comment text.
        body: String,
    },
    /// Submit new feedback to the board.
    SubmitFeedback {
        /// Feature request or bug report.
        kind: FeedbackType,
        /// The submission's title.
        title: String,
        /// The submission's body.
        body: String,
    },
    /// Re-read the declared fleet (roster + capacity) from the runtime.
    RefreshFleet,
    /// Run an installed workflow on this machine.
    ///
    /// Off-thread like every other filesystem/process command: a workflow run
    /// dispatches real agent sessions and takes minutes, so doing it on the
    /// render thread would freeze the app for the whole run.
    #[cfg(feature = "workflows")]
    RunWorkflow {
        /// The workflow to run.
        id: String,
        /// Values for the workflow's declared inputs, collected from the
        /// operator before this command was emitted. Empty when the workflow
        /// declares none.
        inputs: serde_json::Map<String, serde_json::Value>,
    },
    /// Permanently remove an installed workflow after the operator confirms it.
    #[cfg(feature = "workflows")]
    DeleteWorkflow {
        /// The workflow to remove.
        id: String,
    },
    /// Ask the copilot to change or explain a workflow.
    ///
    /// Off-thread for the same reason a run is: the turn starts a real agent
    /// session, and the pane it reports into has to keep repainting while it
    /// does.
    #[cfg(feature = "workflows")]
    CopilotTurn {
        /// The workflow the turn is scoped to.
        workflow: String,
        /// The operator's instruction, verbatim.
        instruction: String,
    },
    /// Ask the copilot to build a workflow that does not exist yet.
    ///
    /// Separate from [`Cmd::CopilotTurn`] because it has no workflow to name:
    /// the agent is told to call `workflow_create`, and which workflow appeared
    /// is worked out from the store afterwards.
    #[cfg(feature = "workflows")]
    CreateWorkflow {
        /// Which copilot thread the turn's progress and result belong to.
        ///
        /// Carried rather than assumed: the thread for a workflow that does not
        /// exist is keyed by a sentinel the app owns, and an event loop that
        /// had to know that sentinel would be a second place it is spelled.
        thread: String,
        /// The operator's description of what they want, verbatim.
        instruction: String,
    },
    /// Simulate a workflow without dispatching anything, and report the result.
    #[cfg(feature = "workflows")]
    DryRunWorkflow {
        /// The workflow to simulate.
        id: String,
        /// Values for the workflow's declared inputs — a simulation resolves
        /// `=inputs.<name>` bindings like a real run, so it needs them too.
        inputs: serde_json::Map<String, serde_json::Value>,
    },
    /// Take back a workflow's most recent edit.
    ///
    /// Off-thread with the rest: it reads the history directory and writes a
    /// definition, and the store's methods are synchronous by contract.
    #[cfg(feature = "workflows")]
    UndoWorkflow {
        /// The workflow to restore.
        id: String,
    },
    /// Stop the copilot turn running on a thread.
    #[cfg(feature = "workflows")]
    AbortCopilot {
        /// Which copilot thread to stop.
        thread: String,
    },
    /// Ask the copilot to diagnose a failed run and fix its cause.
    ///
    /// Separate from [`Cmd::CopilotTurn`] because it carries the failure: the
    /// run, its error, and the nodes implicated. All three are on screen when
    /// the operator presses the key, and a turn that had to rediscover them
    /// would start a step behind.
    #[cfg(feature = "workflows")]
    RepairWorkflow {
        /// The workflow the run belongs to.
        workflow: String,
        /// The operator's words, if they typed any.
        instruction: String,
        /// The run to diagnose.
        run_id: String,
    },
    /// Review a workflow against its own history.
    ///
    /// Unlike [`Cmd::RepairWorkflow`], this turn may not edit: it records what
    /// it learns and proposes changes for the operator to accept. The two are
    /// separate commands rather than one with a flag because they are different
    /// asks — repair is "fix this now", review is "what should change".
    #[cfg(feature = "workflows")]
    EvolveWorkflow {
        /// The workflow to review.
        workflow: String,
        /// The failed run to lead with, when the review was triggered by one.
        run_id: Option<String>,
    },
    /// Apply a proposed change to the saved graph.
    #[cfg(feature = "workflows")]
    AcceptProposal {
        /// The workflow being changed, so the pane can be refreshed.
        workflow: String,
        /// The proposal to apply.
        proposal_id: String,
    },
    /// Turn a proposed change down.
    #[cfg(feature = "workflows")]
    RejectProposal {
        /// The workflow the proposal was for.
        workflow: String,
        /// The proposal to decline.
        proposal_id: String,
        /// Why, recorded as a note so a later review does not propose it again.
        reason: String,
    },
}

/// The Feedback surface's state: the loaded page, the selected row, that row's
/// comments, and the active query.
pub(in crate::ui::app) struct FeedbackState {
    /// The current page of board items.
    pub(in crate::ui::app) items: Vec<FeedbackItem>,
    /// Total items matching the query across all pages.
    pub(in crate::ui::app) total: i64,
    /// The highlighted row.
    pub(in crate::ui::app) index: usize,
    /// Comments for [`FeedbackState::detail_id`], loaded lazily on selection.
    pub(in crate::ui::app) comments: Vec<FeedbackComment>,
    /// Which item [`FeedbackState::comments`] belongs to.
    pub(in crate::ui::app) detail_id: Option<String>,
    /// Scroll offset within the detail pane.
    pub(in crate::ui::app) detail_scroll: usize,
    /// The active filter/sort/pagination.
    pub(in crate::ui::app) query: FeedbackQuery,
    /// Whether the runtime serves a board at all. `false` renders a sign-in
    /// hint instead of an empty list.
    pub(in crate::ui::app) supported: bool,
    /// Whether a board load is in flight (drives the header's "loading…").
    pub(in crate::ui::app) loading: bool,
}

impl Default for FeedbackState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            index: 0,
            comments: Vec::new(),
            detail_id: None,
            detail_scroll: 0,
            query: FeedbackQuery::default(),
            supported: true,
            loading: false,
        }
    }
}

include!("app_state.inc.rs");
