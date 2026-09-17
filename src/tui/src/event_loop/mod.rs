//! The interactive TUI event loop and its async plumbing.
//!
//! Owns [`AppMsg`] (the messages spawned tasks send back to the UI), the main
//! [`run`] select-loop over crossterm events / background messages / a 90ms
//! tick, [`run_cmd`] which turns a [`Cmd`] into a spawned async task, and the
//! background [`spawn_update_checker`]. The loop keeps all mutation on one task
//! and folds async results back in over an mpsc channel.

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::EventStream;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use medulla::backend::Backend;
use medulla::hub::HubSlot;
use medulla_tui::ui::app::{App, Cmd};

use crate::terminal::set_mouse_capture;

mod cmd_dispatch;
mod types;
mod update_checker;

#[cfg(test)]
mod tests;

use types::AppMsg;
pub(crate) use types::{SessionExit, SessionWiring};
use update_checker::spawn_update_checker;

pub(crate) use cmd_dispatch::clear_copilot_hosts;
use cmd_dispatch::run_cmd;

/// Drive the ratatui app: build [`App`] and loop over input events, background
/// [`AppMsg`]s, and the animation tick until the app requests quit.
pub(crate) async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    backend: Arc<dyn Backend>,
    hub: HubSlot,
    wiring: SessionWiring,
) -> anyhow::Result<SessionExit> {
    let SessionWiring {
        offline_demo,
        loaded,
        local_hosts,
        startup_status,
        config_path,
        hooks_config_path,
        medulla_home,
        account,
        mut sharing,
        onboarding_path,
        host,
        local_sessions,
        harness_runs,
        hook_log,
    } = wiring;
    let mut app = App::with_hub(backend.clone(), hub, loaded);
    if offline_demo {
        app.inject_lanes(medulla_tui::ui::agents::demo_lanes());
    }
    app.set_config_path(config_path);
    app.set_hooks_config_path(hooks_config_path);
    app.set_medulla_home(medulla_home);
    app.set_account(account);
    if let Some(host) = host {
        app.set_host_observation(host);
    }
    if let Some(sessions) = local_sessions {
        app.set_local_sessions(sessions);
    }
    app.set_harness_runs(harness_runs);
    app.set_hook_log(hook_log);
    if let Some(status) = startup_status {
        app.set_status(status);
    }
    let mut reader = EventStream::new();
    // The one animation clock. Every frame-driven effect — spinners, the
    // attention pulse — counts in this unit, so the period is declared beside
    // the theme that converts a configured duration into frames.
    let mut tick = tokio::time::interval(Duration::from_millis(medulla_tui::ui::theme::FRAME_MS));
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<AppMsg>();
    let mut mouse_on = true;
    let mut analytics_screen = app.tab();

    // Background release-update checker ("automated cron"): first probe ~10s
    // after startup, then every 6h. A newer version surfaces as a persistent
    // header banner. Disabled via `[update] check = false` or
    // `MEDULLA_NO_UPDATE_CHECK`.
    spawn_update_checker(&app.loaded, &msg_tx);

    loop {
        terminal.draw(|f| app.draw(f))?;
        if app.should_quit {
            break;
        }
        if app.mouse_capture != mouse_on {
            mouse_on = app.mouse_capture;
            set_mouse_capture(mouse_on);
        }

        tokio::select! {
            maybe_event = reader.next() => {
                if let Some(Ok(ev)) = maybe_event {
                    if let Some(cmd) = app.on_event(ev) {
                        run_cmd(
                            cmd,
                            &backend,
                            &app.hub,
                            &app.loaded.config,
                            &msg_tx,
                            local_hosts.as_ref(),
                        );
                    }
                    let screen = app.tab();
                    if screen != analytics_screen {
                        analytics_screen = screen;
                        tokio::spawn(async move {
                            let _ = medulla::analytics::record_screen_view(screen).await;
                        });
                    }
                }
            }
            Some(msg) = msg_rx.recv() => {
                match msg {
                    AppMsg::Status(s) => app.set_status(s),
                    AppMsg::RemoteHostConnected {
                        host_id,
                        capabilities,
                    } => {
                        app.remote_host_connected(&host_id, *capabilities);
                        // Advance the picker if it is still sitting on this
                        // host: the operator pressed Enter and has been waiting
                        // on exactly this answer, so making them press it again
                        // would be asking for a decision they already made.
                        app.resume_picker_host(&host_id);
                    }
                    AppMsg::RemoteHostDialling { host_id, requests } => {
                        app.remote_host_dialling(&host_id, requests);
                    }
                    AppMsg::RemoteHostSessions { host_id, rows } => {
                        app.remote_host_sessions(&host_id, rows);
                    }
                    AppMsg::RemoteSessionOpened {
                        host_id,
                        session_id,
                    } => {
                        // Select it, for the same reason a local spawn does: a
                        // session that appears below the fold while the pane
                        // still shows what was there before reads as "nothing
                        // happened".
                        app.select_remote_session(&host_id, &session_id);
                    }
                    AppMsg::RemoteScreen {
                        host_id,
                        session_id,
                        snapshot,
                    } => {
                        app.remote_host_screen(&host_id, &session_id, *snapshot);
                    }
                    AppMsg::RemoteHostFailed { host_id, reason } => {
                        app.remote_host_failed(&host_id, reason.clone());
                        app.set_status(format!("{host_id}: {reason}"));
                    }
                    AppMsg::RemoteOpenFailed { host_id, reason } => {
                        // The connection itself is fine — only say why the
                        // session did not start, rather than marking the host
                        // as failed and dropping its live sessions from the
                        // rail.
                        app.set_status(format!("{host_id} could not start it: {reason}"));
                    }
                    AppMsg::RemoteWorkflowResult {
                        host_id,
                        request_id,
                        result,
                    } => {
                        app.remote_workflow_result(&host_id, &request_id, result);
                    }
                    AppMsg::RemoteHostLiveness { host_id, liveness } => {
                        app.remote_host_liveness(&host_id, liveness);
                    }
                    AppMsg::HostPairingRun { host_id, result } => {
                        match result {
                            // `Ok` here only means the remote shell accepted
                            // the backgrounded command — it is not proof the
                            // daemon came up (a missing binary, a bad key, or
                            // a taken port would still exit the ssh session
                            // successfully). Liveness is confirmed the same
                            // way it always is: by dialling, which is what
                            // pressing ⏎ does next.
                            Ok(()) => app.set_status(format!(
                                "ssh command sent to {host_id} — press ⏎ on the Hosts tab to connect and confirm pairing"
                            )),
                            Err(reason) => app.set_status(format!("{host_id}: ssh failed: {reason}")),
                        }
                    }
                    AppMsg::UsageLoaded(result) => {
                        let cmd = match result {
                            Ok(data) => app.set_account_usage(data),
                            Err(error) => {
                                app.set_status(format!("usage fetch failed: {error}"));
                                app.usage_load_failed()
                            }
                        };
                        if let Some(cmd) = cmd {
                            run_cmd(
                                cmd,
                                &backend,
                                &app.hub,
                                &app.loaded.config,
                                &msg_tx,
                                local_hosts.as_ref(),
                            );
                        }
                    }
                    AppMsg::SubscriptionsLoaded(snapshot) => {
                        if let Some(cmd) = app.set_subscriptions(*snapshot) {
                            run_cmd(
                                cmd,
                                &backend,
                                &app.hub,
                                &app.loaded.config,
                                &msg_tx,
                                local_hosts.as_ref(),
                            );
                        }
                    }
                    AppMsg::LoggedOut => {
                        app.set_status("Account · logged out. Returning to the login screen…");
                        app.logged_out();
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::CopilotStarted { workflow, instruction } => {
                        app.copilot_started(&workflow, &instruction);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::WorkflowRunStarted { workflow, run_id } => {
                        app.workflow_run_started(&workflow, &run_id);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::WorkflowRunOutput {
                        run_id,
                        node,
                        line,
                        // Dropped here, which is what frees the sink's backlog
                        // slot; see `PendingFrame`.
                        pending: _pending,
                    } => {
                        app.workflow_run_output(&run_id, &node, line);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::WorkflowRunFinished { run_id } => {
                        app.workflow_run_finished(&run_id);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::CopilotStatus { workflow, line } => {
                        app.copilot_status(&workflow, line);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::CopilotDone {
                        workflow,
                        reply,
                        changes,
                        created,
                        removed,
                    } => {
                        // The conversation follows the workflow. A create turn
                        // ran on a sentinel thread because there was no id yet;
                        // now there is one, so the harness session moves onto it
                        // and the operator's follow-up is turn two of the same
                        // conversation rather than turn one of a new one.
                        if let Some(created) = &created {
                            cmd_dispatch::adopt_copilot_host(&workflow, created);
                        }
                        // Nothing left to continue, and no reason to keep a
                        // daemon and its harness processes alive for it.
                        if removed {
                            cmd_dispatch::close_copilot_host(&workflow);
                            // And the saved conversation with it: kept, it
                            // would be handed wholesale to the next workflow
                            // that reused the id.
                            app.forget_copilot(&workflow);
                        }
                        // A queued follow-up comes back as a command to run:
                        // the drain happens after the catalogue refresh, so it
                        // sees the graph this turn left behind.
                        let queued = app.copilot_finished(&workflow, reply, changes, created);
                        if let Some(cmd) = queued {
                            run_cmd(
                                cmd,
                                &backend,
                                &app.hub,
                                &app.loaded.config,
                                &msg_tx,
                                local_hosts.as_ref(),
                            );
                        }
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::CopilotFailed {
                        workflow,
                        instruction,
                        error,
                    } => {
                        app.copilot_failed(&workflow, instruction, error);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::WorkflowDeleted { id } => {
                        apply_workflow_deleted(&mut app, &id, cmd_dispatch::close_copilot_host);
                    }
                    #[cfg(feature = "workflows")]
                    AppMsg::WorkflowsChanged => app.reload_workflows(),
                    AppMsg::FeedbackLoaded { query, page } => {
                        if !app.set_feedback_page(query, page) {
                            continue;
                        }
                        // Pull the newly selected row's comments in the same beat.
                        if let Some(cmd) = app.feedback_detail_cmd() {
                            run_cmd(
                                cmd,
                                &backend,
                                &app.hub,
                                &app.loaded.config,
                                &msg_tx,
                                local_hosts.as_ref(),
                            );
                        }
                    }
                    AppMsg::FeedbackComments { id, comments } => {
                        app.set_feedback_comments(id, comments);
                    }
                    AppMsg::FeedbackItemUpdated(item) => {
                        app.apply_feedback_item(item);
                        app.set_status("Feedback · vote recorded");
                    }
                    AppMsg::FeedbackChanged(status) => {
                        app.set_status(status);
                        // The board is about to be re-read; forgetting which
                        // item's comments are loaded is what makes the reload
                        // fetch them again, so a just-posted comment appears.
                        app.invalidate_feedback_detail();
                        // A comment or submission changes the board, so re-pull
                        // it rather than patching state locally.
                        run_cmd(
                            Cmd::LoadFeedback(app.feedback_query()),
                            &backend,
                            &app.hub,
                            &app.loaded.config,
                            &msg_tx,
                            local_hosts.as_ref(),
                        );
                    }
                    AppMsg::UpdateAvailable(notice) => {
                        app.set_update_notice(notice.clone());
                        app.set_status(notice);
                    }
                }
            }
            // A history share the welcome flow handed over. Reported on the
            // status line so the user sees it land without ever being blocked
            // by it. `recv` on a `None` receiver would be `Poll::Pending`
            // forever, so the arm is disabled outright when nothing is running.
            Some(ev) = async { sharing.as_mut()?.recv().await }, if sharing.is_some() => {
                let status = medulla_tui::ui::welcome::share_status(&ev, || {
                    medulla_tui::ui::welcome::persist_onboarding(&onboarding_path)
                });
                if let Some(status) = status {
                    app.set_status(status);
                }
                // A settled share is the last thing this channel will say. Drop
                // it so the arm stops being polled.
                if medulla_tui::ui::welcome::settles_share(&ev) {
                    sharing = None;
                }
            }
            _ = tick.tick() => {
                // Advanced every tick: it is the whole UI's animation clock,
                // and the workflow canvas's wires flow whether or not anything
                // is running. The spinner it also drives is only drawn while
                // running, so it is unaffected.
                app.frame = app.frame.wrapping_add(1);
                // And the rail's own clock: a harness that exited since the
                // last tick stops being listed, and the record it was holding
                // is dropped with it.
                app.sweep_finished_sessions();
                // A remote host's run list moves on that host; the Workflows
                // tab polls it while a run there is unsettled.
                app.tick_remote_workflows();
                // The subscription meters are sampled from the tick rather than
                // on a timer of their own: `due_subscription_refresh` is the
                // one place that knows whether a sample is switched on, stale,
                // and not already running.
                if let Some(cmd) = app.due_subscription_refresh() {
                    run_cmd(
                        cmd,
                        &backend,
                        &app.hub,
                        &app.loaded.config,
                        &msg_tx,
                        local_hosts.as_ref(),
                    );
                }
            }
        }
    }
    Ok(if app.relogin_requested() {
        SessionExit::Relogin
    } else {
        SessionExit::Quit
    })
}

/// Apply the UI-side consequences after a workflow definition is known to be
/// gone, including shutting down its process-backed copilot conversation.
#[cfg(feature = "workflows")]
fn apply_workflow_deleted(app: &mut App, id: &str, close_host: impl FnOnce(&str)) {
    close_host(id);
    app.forget_copilot(id);
    app.reload_workflows();
}
