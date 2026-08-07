//! Tests for workflow-run rows in the Agents rail.

use super::super::color;
use super::tests::{app, lane, none_waiting, NOW};

/// A reported run, as the control plane hands one to the rail.
fn reported_run(
    status: medulla::control_socket::HarnessRunStatus,
) -> medulla::control_socket::HarnessRun {
    medulla::control_socket::HarnessRun {
        run_id: "run-1".into(),
        workflow_id: "review-and-fix".into(),
        status,
        started_at: 1,
        updated_at: 2,
        detail: Some("review · running the test suite".into()),
        frames: Vec::new(),
    }
}

#[test]
fn a_workflow_run_row_names_its_workflow_status_and_elapsed_time() {
    let app = app();
    let row =
        crate::ui::app::rail::RailRow::WorkflowRun(crate::ui::app::rail::WorkflowRunRailRow {
            session_id: "w_1".into(),
            run: reported_run(medulla::control_socket::HarnessRunStatus::Running),
            last: true,
        });

    let line = app.rail_row_line(&row, &[lane()], false, &none_waiting(), NOW);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(text.contains("review-and-fix"), "{text}");
    assert!(text.contains("running"), "{text}");
    assert!(text.contains("9s"), "{text}");
    assert!(text.starts_with("   └"), "{text}");
}

#[test]
fn a_workflow_run_row_carries_no_harness_output() {
    let app = app();
    let row =
        crate::ui::app::rail::RailRow::WorkflowRun(crate::ui::app::rail::WorkflowRunRailRow {
            session_id: "w_1".into(),
            run: reported_run(medulla::control_socket::HarnessRunStatus::Running),
            last: true,
        });

    let line = app.rail_row_line(&row, &[lane()], false, &none_waiting(), NOW);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(!text.contains("running the test suite"), "{text}");
}

#[test]
fn a_settled_run_row_stops_ageing_at_its_last_report() {
    let mut run = reported_run(medulla::control_socket::HarnessRunStatus::Succeeded);
    run.started_at = 1_000;
    run.updated_at = 4_000;
    let app = app();
    let row =
        crate::ui::app::rail::RailRow::WorkflowRun(crate::ui::app::rail::WorkflowRunRailRow {
            session_id: "w_1".into(),
            run,
            last: true,
        });

    let line = app.rail_row_line(&row, &[lane()], false, &none_waiting(), NOW + 600_000);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(text.contains("3s"), "{text}");
}

#[test]
fn a_failed_run_row_is_coloured_by_its_status_rather_than_by_the_row() {
    let app = app();
    let row =
        crate::ui::app::rail::RailRow::WorkflowRun(crate::ui::app::rail::WorkflowRunRailRow {
            session_id: "w_1".into(),
            run: reported_run(medulla::control_socket::HarnessRunStatus::Failed),
            last: false,
        });

    let line = app.rail_row_line(&row, &[lane()], false, &none_waiting(), NOW);
    let status = line
        .spans
        .iter()
        .find(|span| span.content.contains("failed"))
        .expect("a status span");
    assert_eq!(status.style.fg, Some(color("red")));
}
