//! Tests for the capability probe: the hub asks a worker for its budgets and
//! readiness over the encrypted relay, and a cancelled probe leaves nothing
//! behind.

use std::time::Duration;

use crate::hub::tests::dispatch::harness::{FakeWorker, Mode};
use crate::hub::TaskRunner;
use crate::protocol::{
    AgentCapabilities, BudgetSource, BudgetWindow, HarnessBudget, HarnessProvider, HarnessReadiness,
};

fn caps() -> AgentCapabilities {
    AgentCapabilities {
        providers: vec![HarnessProvider::Claude],
        budgets: vec![HarnessBudget {
            provider: HarnessProvider::Claude,
            seat: Some("seat-1".to_string()),
            window: BudgetWindow::FiveHour,
            limit_tokens: Some(1_000),
            used_tokens: Some(250),
            remaining_tokens: Some(750),
            cooldown_until: None,
            source: BudgetSource::Estimate,
        }],
        readiness: vec![HarnessReadiness {
            provider: HarnessProvider::Claude,
            ready: true,
            reason: None,
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn capability_probe_returns_the_workers_budgets_and_readiness() {
    let worker = FakeWorker::new(Mode::Capabilities(Box::new(caps())));
    let runner = TaskRunner::start_with_ack_window(
        worker.clone(),
        Duration::from_millis(5),
        Duration::from_millis(100),
    );

    let result = runner
        .capabilities("worker")
        .await
        .expect("capability reply");

    assert_eq!(result, caps());
    assert_eq!(worker.sent_kinds().await, vec!["capabilities"]);
}

#[tokio::test]
async fn cancelling_a_capability_probe_reaps_its_waiter() {
    let worker = FakeWorker::new(Mode::Silent);
    let runner = TaskRunner::start_with_ack_window(
        worker.clone(),
        Duration::from_millis(5),
        Duration::from_secs(30),
    );

    let result =
        tokio::time::timeout(Duration::from_millis(50), runner.capabilities("worker")).await;

    assert!(result.is_err(), "the outer deadline must cancel the probe");
    assert_eq!(worker.sent_kinds().await, vec!["capabilities"]);
    assert_eq!(runner.capability_waiter_count(), 0);
}
