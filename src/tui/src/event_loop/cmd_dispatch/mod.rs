//! Translate a [`Cmd`] into a spawned async task whose result is reported back
//! over the [`AppMsg`] channel.
//!
//! Extracted from the main [`super::event_loop`] module so it stays under the
//! repository's 500-line ceiling.

use std::sync::Arc;

use medulla::backend::Backend;
use medulla::hub::{apply_worker_op, hub_in, HubHandle, HubSlot};
use medulla_tui::ui::app::Cmd;

use super::AppMsg;

#[cfg(feature = "workflows")]
mod copilot_hosts;
mod feedback;
mod remote_hosts;
#[cfg(feature = "workflows")]
mod workflow_evolution;
#[cfg(feature = "workflows")]
mod workflows;

/// Move a copilot conversation onto the id of the workflow it just created.
///
/// Exposed here rather than reached into directly so the event loop has one
/// door into the host cache, matching how it reaches every other spawned
/// concern in this module.
#[cfg(feature = "workflows")]
pub(super) fn adopt_copilot_host(thread: &str, created: &str) {
    copilot_hosts::rename(thread, created);
}

/// End a copilot conversation whose workflow no longer exists.
#[cfg(feature = "workflows")]
pub(super) fn close_copilot_host(thread: &str) {
    copilot_hosts::abort(thread);
    copilot_hosts::forget(thread);
}

/// Drop every cached copilot host and stop its daemon.
///
/// The cache is process-global and keyed by workflow id, not by account — see
/// [`copilot_hosts::clear_all`] for why that makes a relogin the one place
/// this must be called.
#[cfg(feature = "workflows")]
pub(crate) fn clear_copilot_hosts() {
    copilot_hosts::clear_all();
}

/// No-op build without the `workflows` feature: there is no host cache to
/// clear, but the relogin call site is unconditional.
#[cfg(not(feature = "workflows"))]
pub(crate) fn clear_copilot_hosts() {}

#[cfg(test)]
mod tests;

/// Translate a [`Cmd`] emitted by the app into a spawned async task whose result
/// is reported back over the [`AppMsg`] channel.
///
/// `hub` is read per command rather than captured: the slot is filled after
/// startup and refilled by nothing, but a command issued before the hub is up
/// has to be told so rather than silently succeed at a mutation that did not
/// happen.
///
/// `config` is the app's already-loaded configuration, threaded in for the
/// commands that dispatch a harness off-thread ([`Cmd::RunWorkflow`],
/// [`Cmd::CopilotTurn`] and the rest) so they use the config the TUI actually
/// started with — including an explicit `--config` — rather than rediscovering
/// defaults from a fresh load. The whole document rather than its `[workflows]`
/// section alone, because those commands each start an embedded host and need
/// the launch policy — attribution and `[[hooks]]` — that lives outside it.
/// Named with a leading underscore because a build without the `workflows`
/// feature has no arm that reads it.
pub(super) fn run_cmd(
    cmd: Cmd,
    backend: &Arc<dyn Backend>,
    hub: &HubSlot,
    _config: &medulla::config::TuiConfig,
    msg_tx: &tokio::sync::mpsc::UnboundedSender<AppMsg>,
    local_hosts: Option<&crate::local_host::LocalHostHarnesses>,
) {
    let cmd = match feedback::run_feedback_cmd(cmd, backend, msg_tx) {
        Some(cmd) => *cmd,
        None => return,
    };
    match cmd {
        Cmd::Quit => {}
        Cmd::LoadFeedback(_)
        | Cmd::LoadFeedbackDetail(_)
        | Cmd::VoteFeedback { .. }
        | Cmd::CommentFeedback { .. }
        | Cmd::SubmitFeedback { .. } => {
            unreachable!("feedback commands return before main dispatch")
        }
        Cmd::Logout => {
            let rt = backend.clone();
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                // Only a clear that actually landed ends the session: reporting
                // success early would drop the operator back at the login screen
                // still signed in.
                let msg = match rt.logout().await {
                    Ok(()) => AppMsg::LoggedOut,
                    Err(e) => AppMsg::Status(format!("Account · logout failed: {e}")),
                };
                let _ = tx.send(msg);
            });
        }
        Cmd::RunPairingOverSsh {
            host_id,
            section,
            command,
        } => {
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let result =
                    medulla_tui::remote::client::autorun::run_pairing(&section, &command).await;
                let _ = tx.send(AppMsg::HostPairingRun { host_id, result });
            });
        }
        Cmd::ConnectRemoteHost { host_id } => {
            if let Some(requests) = remote_hosts::spawn_connect(host_id.clone(), _config, msg_tx) {
                // Handed straight back to the App: it is the only thing that can
                // hold the channel, and without it the connection would be up
                // with nothing able to ask it for anything.
                let _ = msg_tx.send(AppMsg::RemoteHostDialling { host_id, requests });
            }
        }
        Cmd::WatchTask { stop, start } => {
            let hub = hub_in(hub);
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                // Stop first: a worker streaming a task nobody is looking at
                // spends a sample, a ratchet advance and a send every tick.
                // Stopping with no hub is a no-op — there is genuinely nothing
                // to stop — so only the request to *start* one is an error
                // worth surfacing.
                if let (Some(hub), Some((worker, task_id))) = (&hub, stop) {
                    let _ = hub.unwatch(&worker, &task_id).await;
                }
                if let Some((worker, task_id)) = start {
                    // Reported rather than swallowed. A silent success here is
                    // the worst version of this failure: the pane stays empty,
                    // the worker is never asked for anything, and nothing
                    // anywhere says why.
                    let result = match &hub {
                        Some(hub) => hub.watch(&worker, &task_id).await,
                        None => Err(no_hub("this worker cannot be asked to stream its screen")),
                    };
                    if let Err(e) = result {
                        let _ = tx.send(AppMsg::Status(format!("Cannot watch {task_id}: {e}")));
                    }
                }
            });
        }
        Cmd::KillTask { worker, task_id } => {
            let hub = hub_in(hub);
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let result = match &hub {
                    Some(hub) => hub.kill(&worker, &task_id).await,
                    None => Err(no_hub("nothing can be killed")),
                };
                let status = match result {
                    Ok(()) => format!("Kill requested for {task_id}"),
                    Err(e) => format!("Cannot kill {task_id}: {e}"),
                };
                let _ = tx.send(AppMsg::Status(status));
            });
        }
        Cmd::WorkerOp(op) => {
            let hub = hub_in(hub);
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let status = match worker_op(hub.as_ref(), op).await {
                    Ok(()) => "Worker registry updated".to_string(),
                    Err(e) => e.to_string(),
                };
                let _ = tx.send(AppMsg::Status(status));
            });
        }
        Cmd::WorkerOps(ops) => {
            let hub = hub_in(hub);
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let total = ops.len();
                let mut applied = 0usize;
                let mut failure = None;
                for op in ops {
                    // Stop at the first failure rather than pressing on. The
                    // ops are one operator action, and continuing past a
                    // refusal would half-remove a host — some agents gone, the
                    // rest still routed to — which is the state hardest to
                    // reason about from the screen.
                    match worker_op(hub.as_ref(), op).await {
                        Ok(()) => applied += 1,
                        Err(e) => {
                            failure = Some(e.to_string());
                            break;
                        }
                    }
                }
                let status = match failure {
                    None => "Worker registry updated".to_string(),
                    Some(e) if applied == 0 => e,
                    // Says what landed as well as what stopped it: the operator
                    // is looking at a list that is now partly changed.
                    Some(e) => format!("Removed {applied} of {total}, then: {e}"),
                };
                let _ = tx.send(AppMsg::Status(status));
            });
        }
        #[cfg(feature = "workflows")]
        Cmd::RunWorkflow { id, inputs } => {
            let custom_harnesses = local_hosts
                .map(|spawner| spawner.custom_harnesses().to_vec())
                .unwrap_or_default();
            workflows::spawn_run(
                id,
                inputs,
                _config.workflows.clone(),
                custom_harnesses,
                medulla::harness_hooks::LaunchPolicy::from_config(_config),
                msg_tx,
            )
        }
        #[cfg(feature = "workflows")]
        Cmd::DeleteWorkflow { id } => workflows::spawn_delete(id, msg_tx),
        #[cfg(feature = "workflows")]
        Cmd::DryRunWorkflow { id, inputs } => workflows::spawn_dry_run(id, inputs, msg_tx),
        #[cfg(feature = "workflows")]
        Cmd::UndoWorkflow { id } => workflows::spawn_undo(id, msg_tx),
        #[cfg(feature = "workflows")]
        Cmd::AbortCopilot { thread } => {
            // Inline: signalling an abort is a lock and a notify, and spawning
            // a task to do it would only delay the one thing the operator is
            // waiting for.
            let status = if copilot_hosts::abort(&thread) {
                "Stopping the copilot…"
            } else {
                "Nothing is running on this thread"
            };
            let _ = msg_tx.send(AppMsg::Status(status.to_string()));
        }
        #[cfg(feature = "workflows")]
        Cmd::RepairWorkflow {
            workflow,
            instruction,
            run_id,
        } => workflows::spawn_repair(
            workflow,
            instruction,
            run_id,
            _config.workflows.clone(),
            medulla::harness_hooks::LaunchPolicy::from_config(_config),
            msg_tx,
        ),
        #[cfg(feature = "workflows")]
        Cmd::EvolveWorkflow { workflow, run_id } => workflow_evolution::spawn_evolve(
            workflow,
            run_id,
            _config.workflows.clone(),
            medulla::harness_hooks::LaunchPolicy::from_config(_config),
            msg_tx,
        ),
        #[cfg(feature = "workflows")]
        Cmd::AcceptProposal {
            workflow,
            proposal_id,
        } => workflows::spawn_decision(workflow, proposal_id, None, msg_tx),
        #[cfg(feature = "workflows")]
        Cmd::RejectProposal {
            workflow,
            proposal_id,
            reason,
        } => workflows::spawn_decision(workflow, proposal_id, Some(reason), msg_tx),
        #[cfg(feature = "workflows")]
        Cmd::CopilotTurn {
            workflow,
            instruction,
        } => workflows::spawn_copilot(
            workflow,
            instruction,
            _config.workflows.clone(),
            medulla::harness_hooks::LaunchPolicy::from_config(_config),
            msg_tx,
        ),
        #[cfg(feature = "workflows")]
        Cmd::CreateWorkflow {
            thread,
            instruction,
        } => workflows::spawn_copilot_create(
            thread,
            instruction,
            _config.workflows.clone(),
            medulla::harness_hooks::LaunchPolicy::from_config(_config),
            msg_tx,
        ),
        Cmd::LoadSubscriptions(account) => {
            let config = _config.subscriptions.clone();
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let snapshot = medulla::subscriptions::sample(&config, account.as_ref()).await;
                let _ = tx.send(AppMsg::SubscriptionsLoaded(Box::new(snapshot)));
            });
        }
        Cmd::LoadUsage => {
            let rt = backend.clone();
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let result = rt.team_usage().await.map_err(|error| error.to_string());
                let _ = tx.send(AppMsg::UsageLoaded(result));
            });
        }
    }
}

/// Apply a worker mutation on the hub, or explain why it cannot be applied.
///
/// Reading an empty roster is honest; silently succeeding at a *mutation* that
/// did not happen is not. Without a hub there is nothing to add a worker to,
/// and reporting "updated" leaves the operator watching for a peer that was
/// never registered.
async fn worker_op(hub: Option<&HubHandle>, op: medulla::fleet::WorkerOp) -> anyhow::Result<()> {
    match hub {
        Some(hub) => apply_worker_op(hub, op).await,
        None => Err(anyhow::anyhow!(
            "no hub is running — restart, or unset MEDULLA_HUB=0"
        )),
    }
}

/// The error a hub-side command reports when the hub never started.
fn no_hub(consequence: &str) -> String {
    format!("no hub is running, so {consequence}")
}
