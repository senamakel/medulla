//! Command-dispatch coverage for every asynchronous command family.

use std::sync::Arc;
use std::time::Duration;

use medulla::backend::{Backend, MockBackend};
use medulla::client::{FeedbackQuery, FeedbackType};
use medulla::fleet::WorkerOp;
use medulla::hub::HubSlot;
use medulla_tui::ui::app::Cmd;

use super::run_cmd as dispatch_cmd;
use crate::event_loop::types::AppMsg;

#[cfg(feature = "workflows")]
use super::workflows::{report_delete, DeleteOutcome};

/// Dispatch through the production entry point with default workflow settings
/// and no hub running.
fn run_cmd(
    cmd: Cmd,
    backend: &Arc<dyn Backend>,
    _: Option<()>,
    tx: &tokio::sync::mpsc::UnboundedSender<AppMsg>,
) {
    dispatch_cmd(
        cmd,
        backend,
        &HubSlot::default(),
        &medulla::config::TuiConfig::default(),
        tx,
        None,
    );
}

/// Receive a spawned command result through a finite test boundary.
async fn next(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppMsg>) -> AppMsg {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("dispatcher timed out")
        .expect("dispatcher dropped its response channel")
}

/// Assert the status followed by the optional catalogue-removal notification.
#[cfg(feature = "workflows")]
fn assert_delete_messages(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppMsg>,
    status_fragment: &str,
    deleted: bool,
) {
    match rx.try_recv().expect("deletion status") {
        AppMsg::Status(status) => assert!(status.contains(status_fragment), "{status}"),
        _ => panic!("deletion must report status first"),
    }
    if deleted {
        assert!(matches!(
            rx.try_recv().expect("workflow deletion notification"),
            AppMsg::WorkflowDeleted { id } if id == "sweep"
        ));
    }
    assert!(rx.try_recv().is_err(), "deletion sent an extra message");
}

#[cfg(feature = "workflows")]
#[test]
fn successful_delete_reports_catalogue_removal() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    report_delete("sweep".into(), Ok(DeleteOutcome::Deleted), &tx);

    assert_delete_messages(&mut rx, "Deleted workflow sweep", true);
}

#[cfg(feature = "workflows")]
#[test]
fn delete_with_undo_warning_still_reports_catalogue_removal() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let warning = medulla::workflows::WorkflowError::Engine("revision unavailable".into());

    report_delete(
        "sweep".into(),
        Ok(DeleteOutcome::DeletedWithWarning(warning)),
        &tx,
    );

    assert_delete_messages(&mut rx, "could not record undo history", true);
}

#[cfg(feature = "workflows")]
#[test]
fn failed_delete_does_not_report_catalogue_removal() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let failure = medulla::workflows::WorkflowError::Engine("definition retained".into());

    report_delete("sweep".into(), Ok(DeleteOutcome::Failed(failure)), &tx);

    assert_delete_messages(&mut rx, "Could not delete workflow sweep", false);
}

#[cfg(feature = "workflows")]
#[tokio::test]
async fn delete_task_join_failure_does_not_report_catalogue_removal() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let join_error = tokio::spawn(async { panic!("injected task failure") })
        .await
        .expect_err("panicking task must fail to join");

    report_delete("sweep".into(), Err(join_error), &tx);

    assert_delete_messages(&mut rx, "Could not delete workflow sweep", false);
}

struct FailingBackend;

impl FailingBackend {
    fn failure<T: Send + 'static>() -> futures::future::BoxFuture<'static, anyhow::Result<T>> {
        Box::pin(async { Err(anyhow::anyhow!("injected backend failure")) })
    }
}

impl Backend for FailingBackend {
    fn describe(&self) -> String {
        "failing test backend".into()
    }

    fn team_usage(
        &self,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Option<serde_json::Value>>> {
        Self::failure()
    }

    fn logout(&self) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Self::failure()
    }

    fn list_feedback(
        &self,
        _: FeedbackQuery,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Option<medulla::client::FeedbackPage>>>
    {
        Self::failure()
    }
}

#[tokio::test]
async fn dispatches_fleet_and_usage_commands() {
    let backend: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    run_cmd(Cmd::Quit, &backend, None, &tx);
    assert!(rx.try_recv().is_err());

    // No hub is running, so a roster mutation is refused rather than reported
    // as applied — see `worker_op`.
    run_cmd(
        Cmd::WorkerOp(WorkerOp::Add {
            address: Some("peer-1".into()),
            handle: None,
            label: Some("Peer".into()),
            harness: None,
        }),
        &backend,
        None,
        &tx,
    );
    assert!(matches!(next(&mut rx).await, AppMsg::Status(s) if s.contains("no hub is running")));

    run_cmd(Cmd::LoadUsage, &backend, None, &tx);
    assert!(matches!(next(&mut rx).await, AppMsg::UsageLoaded(Ok(None))));
}

#[tokio::test]
async fn dispatches_every_feedback_action() {
    let runtime: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    run_cmd(
        Cmd::LoadFeedback(FeedbackQuery::default()),
        &runtime,
        None,
        &tx,
    );
    assert!(matches!(
        next(&mut rx).await,
        AppMsg::FeedbackLoaded { page: Some(page), .. } if !page.items.is_empty()
    ));

    run_cmd(Cmd::LoadFeedbackDetail("fb-2".into()), &runtime, None, &tx);
    assert!(matches!(
        next(&mut rx).await,
        AppMsg::FeedbackComments { id, comments } if id == "fb-2" && !comments.is_empty()
    ));

    run_cmd(
        Cmd::VoteFeedback {
            id: "fb-2".into(),
            value: 1,
        },
        &runtime,
        None,
        &tx,
    );
    assert!(matches!(next(&mut rx).await, AppMsg::FeedbackItemUpdated(item) if item.id == "fb-2"));

    run_cmd(
        Cmd::CommentFeedback {
            id: "fb-2".into(),
            body: "Useful".into(),
        },
        &runtime,
        None,
        &tx,
    );
    assert!(matches!(next(&mut rx).await, AppMsg::FeedbackChanged(s) if s.contains("comment")));

    run_cmd(
        Cmd::SubmitFeedback {
            kind: FeedbackType::Feature,
            title: "New feature".into(),
            body: "Please add it".into(),
        },
        &runtime,
        None,
        &tx,
    );
    assert!(matches!(next(&mut rx).await, AppMsg::FeedbackChanged(s) if s.contains("submitted")));
}

#[tokio::test]
async fn dispatcher_surfaces_feedback_errors() {
    let runtime: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    for cmd in [
        Cmd::LoadFeedbackDetail("missing".into()),
        Cmd::VoteFeedback {
            id: "missing".into(),
            value: 1,
        },
        Cmd::CommentFeedback {
            id: "missing".into(),
            body: "nope".into(),
        },
    ] {
        run_cmd(cmd, &runtime, None, &tx);
        assert!(matches!(next(&mut rx).await, AppMsg::Status(s) if s.contains("not found")));
    }
}

#[tokio::test]
async fn dispatcher_surfaces_every_backend_failure() {
    let runtime: Arc<dyn Backend> = Arc::new(FailingBackend);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let commands = [
        Cmd::Logout,
        Cmd::LoadUsage,
        Cmd::LoadFeedback(FeedbackQuery::default()),
        Cmd::LoadFeedbackDetail("fail".into()),
        Cmd::VoteFeedback {
            id: "fail".into(),
            value: 1,
        },
        Cmd::CommentFeedback {
            id: "fail".into(),
            body: "fail".into(),
        },
        Cmd::SubmitFeedback {
            kind: FeedbackType::Bug,
            title: "fail".into(),
            body: "fail".into(),
        },
    ];

    for command in commands {
        run_cmd(command, &runtime, None, &tx);
        let message = next(&mut rx).await;
        let status = match message {
            AppMsg::Status(status) | AppMsg::UsageLoaded(Err(status)) => status,
            _ => panic!("backend failure returned a non-error message"),
        };
        assert!(
            status.contains("injected backend failure")
                || status.contains("requires a signed-in backend"),
            "{status}"
        );
    }
}
