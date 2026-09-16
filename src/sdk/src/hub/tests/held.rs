//! Deferring a task because a person is working in the checkout.
//!
//! Narrower than it was. A dispatch no longer refuses on meeting a person —
//! their session is simply not a candidate, and a dispatch with nothing else to
//! run in queues behind the checkout's writer. `RunError::Held` is what that
//! queue reports when it outlives the caller's budget, which is the one path
//! left to it and the reason the shape below is unchanged: the backend already
//! reads `harnessHeld` as retryable, and a task that was never attempted must
//! not come back as one that failed.
//!
//! The daemon can only say so in the text of an `error` frame, so
//! [`settle`](super::super::runner) has to recognise that text as occupancy
//! rather than as a task that failed, or a dispatcher can only guess whether
//! waiting or routing elsewhere is the right move.

use std::time::Duration;

use super::super::types::{RunError, TaskOutcome};
use super::super::TaskRunner;
use super::dispatch::harness::{req, FakeWorker, Mode};

/// The refusal a daemon sends when an operator holds the workspace.
fn held_message() -> String {
    format!(
        "{}: an operator is working in /repos/acme (codex session w_3)",
        crate::daemon::HARNESS_HELD_PREFIX
    )
}

#[tokio::test]
async fn an_operator_hold_settles_as_held_not_as_a_worker_error() {
    // It arrives as an ordinary `error` frame, exactly like backpressure did
    // before it was told apart. Left as `RunError::Worker` it would reach the
    // orchestrator as a task that was attempted and failed — so the work would
    // be abandoned rather than picked up again when the person is done.
    let message = held_message();
    let worker = FakeWorker::new(Mode::Error(message.clone()));
    let runner = TaskRunner::start(worker, Duration::from_millis(5));

    let err = runner.run(req("x"), None).await.expect_err("is refused");
    assert_eq!(err, RunError::Held(message.clone()));
    assert!(
        is_retryable(&err),
        "nothing was attempted, so the task is deferred rather than failed"
    );
    // The daemon's own wording survives, so whoever reads the failure still sees
    // which workspace was occupied and by which session.
    assert_eq!(
        err.to_string(),
        format!("harness held by operator: {message}")
    );
}

