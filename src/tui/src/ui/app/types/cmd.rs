//! The event-loop command vocabulary: what the screen asks the async side to do.
//!
//! Split from [`model`](super::model) so the App struct and the commands it
//! emits each fit under the file ceiling; nothing here is more than data.

use medulla::runtime::WorkerOp;

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
    /// Fetch account-level usage from the backend for the Usage tab.
    LoadUsage,
    /// Re-sample the subscription-usage meters that are switched on.
    ///
    /// Carries the account-usage document the Usage page loaded, since the
    /// portal meter reads its balance from that rather than issuing a second
    /// request; `None` when it has not been fetched yet.
    LoadSubscriptions(Option<serde_json::Value>),
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
