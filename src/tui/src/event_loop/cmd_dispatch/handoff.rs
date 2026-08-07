//! Sending a handoff brief, off the UI thread.
//!
//! Two things here must not happen on the render thread: reading the git branch
//! shells out to a child process, and the emit itself awaits a socket. Both are
//! fast in the ordinary case and neither is bounded in the bad one.
//!
//! Every outcome is narrated. A brief that silently fails to send is precisely
//! the failure this whole feature exists to remove — the operator would be told
//! the harness was handed back, believe the orchestrator had their note, and
//! never find out otherwise.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use medulla::runtime::Runtime;
use medulla_tui::ui::app::Cmd;

use super::super::AppMsg;

/// Queue one workspace ownership mutation after its earlier mutation.
///
/// The event loop submits commands in UI order but executes their backend calls
/// asynchronously. The completion watch preserves order per workspace without
/// making unrelated workspaces wait for one another.
fn queue_ownership(
    workspace: &str,
) -> (
    Option<tokio::sync::watch::Receiver<bool>>,
    tokio::sync::watch::Sender<bool>,
) {
    static QUEUES: OnceLock<Mutex<HashMap<String, tokio::sync::watch::Receiver<bool>>>> =
        OnceLock::new();
    let (complete, receiver) = tokio::sync::watch::channel(false);
    let mut queues = QUEUES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("ownership queue lock is not poisoned");
    let previous = queues.insert(workspace.to_string(), receiver);
    (previous, complete)
}

/// Wait for an earlier ownership mutation, even if its task ended in failure.
async fn wait_for_prior(mut previous: Option<tokio::sync::watch::Receiver<bool>>) {
    let Some(previous) = previous.as_mut() else {
        return;
    };
    if !*previous.borrow() {
        let _ = previous.changed().await;
    }
}

/// Spawn a handoff command, returning anything else to the caller.
pub(super) fn run_handoff_cmd(
    cmd: Cmd,
    runtime: &Arc<dyn Runtime>,
    msg_tx: &tokio::sync::mpsc::UnboundedSender<AppMsg>,
) -> Option<Box<Cmd>> {
    match cmd {
        Cmd::HandOffSession(brief) => {
            let (previous, complete) = queue_ownership(&brief.workspace_path);
            let runtime = runtime.clone();
            let status_tx = msg_tx.clone();
            tokio::spawn(async move {
                wait_for_prior(previous).await;
                let mut brief = *brief;
                let facts =
                    medulla::daemon::capabilities::read_git_facts(&brief.workspace_path).await;
                brief.branch = facts.branch;
                brief.project = facts.project;
                let message = match runtime.hand_off_harness(brief).await {
                    Ok(()) => "Handed back · the orchestrator has your brief".to_string(),
                    Err(error) => format!(
                        "Handed back · your brief did not send: {error} \
                         (the orchestrator may pick it up without context)"
                    ),
                };
                let _ = status_tx.send(AppMsg::Status(message));
                let _ = complete.send(true);
            });
            None
        }
        Cmd::HoldSession { workspace, reason } => {
            let (previous, complete) = queue_ownership(&workspace);
            let runtime = runtime.clone();
            let status_tx = msg_tx.clone();
            tokio::spawn(async move {
                wait_for_prior(previous).await;
                if let Err(error) = runtime.hold_harness(workspace, reason).await {
                    let _ = status_tx.send(AppMsg::Status(format!(
                        "You have this harness · the orchestrator was not told: {error}"
                    )));
                }
                let _ = complete.send(true);
            });
            None
        }
        other => Some(Box::new(other)),
    }
}
