//! Rendering tests for the Workflows tab.
//!
//! Every test draws the whole tab onto a [`TestBackend`] and reads the resulting
//! screen, because the thing under test is what an operator sees: that the three
//! panes are all present, that the graph is drawn as marked nodes joined by
//! wires, that its wires carry a moving flow highlight, and that a run overlaid
//! on it marks the steps it reached.

use std::sync::Arc;

use medulla::backend::Backend;
use medulla::backend::MockBackend;
use medulla::config::LoadedConfig;
use medulla::ui::workflows::Move;
use medulla::workflows::{RunRecord, RunStatus, RunStep, WorkflowRecord, WorkflowStore};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serde_json::json;

use super::super::super::types::{App, WorkflowFocus, TABS};

/// A workflow branching two ways and merging again.
fn diamond(id: &str) -> WorkflowRecord {
    WorkflowRecord {
        id: id.to_string(),
        name: format!("{id} sweep"),
        description: String::new(),
        enabled: true,
        defaults: Default::default(),
        graph: serde_json::from_value(json!({
            "name": id,
            "nodes": [
                { "id": "start", "kind": "trigger", "name": "Start",
                  "config": { "trigger_kind": "manual" } },
                { "id": "check", "kind": "condition", "name": "Check",
                  "config": { "expression": "=.ok" } },
                { "id": "yes", "kind": "agent", "name": "Yes", "config": { "prompt": "go" } },
                { "id": "no", "kind": "agent", "name": "No", "config": { "prompt": "stop" } },
                { "id": "join", "kind": "merge", "name": "Join", "config": { "inputs": ["a"] } },
            ],
            "edges": [
                { "from_node": "start", "to_node": "check" },
                { "from_node": "check", "from_port": "true", "to_node": "yes" },
                { "from_node": "check", "from_port": "false", "to_node": "no" },
                { "from_node": "yes", "to_node": "join" },
                { "from_node": "no", "to_node": "join" },
            ],
        }))
        .expect("graph parses"),
        source_path: None,
    }
}

/// A workflow with enough parallel lanes to require vertical graph scrolling.
fn fanout(id: &str) -> WorkflowRecord {
    let nodes = std::iter::once(json!({
        "id": "start", "kind": "trigger", "name": "Start",
        "config": { "trigger_kind": "manual" }
    }))
    .chain((0..8).map(|index| {
        json!({
            "id": format!("agent-{index}"),
            "kind": "agent",
            "name": format!("Agent {index}"),
            "config": { "prompt": "go" }
        })
    }))
    .collect::<Vec<_>>();
    let edges = (0..8)
        .map(|index| json!({ "from_node": "start", "to_node": format!("agent-{index}") }))
        .collect::<Vec<_>>();
    WorkflowRecord {
        id: id.to_string(),
        name: format!("{id} fanout"),
        description: String::new(),
        enabled: true,
        defaults: Default::default(),
        graph: serde_json::from_value(json!({
            "name": id,
            "nodes": nodes,
            "edges": edges,
        }))
        .expect("graph parses"),
        source_path: None,
    }
}

/// An app on the Workflows tab, with `workflows` installed and `runs` recorded.
fn app_with(workflows: &[WorkflowRecord], runs: &[RunRecord]) -> (tempfile::TempDir, App) {
    let home = tempfile::tempdir().expect("tempdir");
    let runtime: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let mut app = App::new(runtime, LoadedConfig::defaults("medulla.tui.json".into()));
    app.set_medulla_home(home.path().to_path_buf());
    // Only the temp directory, so the catalogue is exactly what this test
    // installed rather than that plus whatever the checkout happens to hold.
    let store: Arc<dyn WorkflowStore> = Arc::new(medulla::workflows::FileWorkflowStore::new(
        vec![home.path().join("workflows")],
        home.path().join("runs"),
    ));
    app.set_workflow_store(store.clone());
    for workflow in workflows {
        store.save(workflow).expect("save");
    }
    for run in runs {
        store.record_run(run).expect("record");
    }
    app.tab_index = TABS
        .iter()
        .position(|tab| *tab == "Workflows")
        .expect("the tab exists");
    app.reload_workflows();
    (home, app)
}

/// The whole screen, as one string.
fn render(app: &mut App) -> String {
    render_sized(app, 160, 40)
}

/// The whole screen at a given size, as one string.
fn render_sized(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|f| app.draw(f)).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    // Row-wise, so a test can look for a whole line rather than a wrap artefact.
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn workflows_is_a_top_level_tab() {
    assert!(TABS.contains(&"Workflows"));
}

#[test]
fn the_tab_is_a_sidebar_beside_one_content_pane() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    let screen = render(&mut app);

    assert!(screen.contains("Workflows · 1"), "the rail is missing");
    assert!(
        screen.contains("nightly sweep"),
        "the canvas title is missing"
    );
    // One view at a time: the copilot is a keystroke away, not a third column
    // competing with the graph for the width.
    assert!(!screen.contains("Copilot"), "{screen}");
}

#[test]
fn canvas_navigation_uses_the_split_graph_panes_measured_height() {
    let (_home, mut app) = app_with(&[fanout("parallel")], &[]);

    // Short enough that eight parallel lanes cannot all be on screen: the point
    // of the test is that navigation scrolls by what the *graph pane* measured,
    // and a pane tall enough for the whole fanout never has to scroll at all.
    render_sized(&mut app, 140, 16);
    let measured_rows = app.visible_rows();
    assert_eq!(measured_rows, app.wf.graph_rows.max(1));

    app.move_graph_cursor(Move::Forward);
    for _ in 0..7 {
        app.move_graph_cursor(Move::LaneDown);
    }
    let node = app.selected_graph_node().expect("selected node").clone();
    let (_, row) = app.graph_cell(node.layer, node.lane);
    assert!(
        row >= app.wf.canvas_row && row < app.wf.canvas_row + measured_rows,
        "row {row} must remain inside {}..{}",
        app.wf.canvas_row,
        app.wf.canvas_row + measured_rows
    );
    assert!(app.wf.canvas_row > 0, "the reduced pane must scroll");
}

#[test]
fn each_view_takes_the_content_pane_in_turn() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    // The graph is what the tab falls back to.
    assert!(render(&mut app).contains("▶ Start"));

    app.wf.inspector_open = true;
    let inspector = render(&mut app);
    assert!(inspector.contains("i back to the graph"), "{inspector}");
    assert!(!inspector.contains("▶ Start"), "one view at a time");

    // The copilot wins over the inspector: it is the one you step into.
    app.wf.focus = WorkflowFocus::Copilot;
    let copilot = render(&mut app);
    assert!(copilot.contains("Copilot"), "{copilot}");
    assert!(
        !copilot.contains("i back to the graph"),
        "one view at a time"
    );

    // Stepping into a view hands the rail's columns to that view — a catalogue
    // of one is twenty-eight columns of nothing beside a graph that needs them.
    // Esc is still the way back, and brings the list with it.
    assert!(!copilot.contains("Workflows · 1"), "{copilot}");
    app.wf.focus = WorkflowFocus::Sidebar;
    let list = render(&mut app);
    assert!(list.contains("Workflows · 1"), "{list}");
}

#[test]
fn long_workflow_names_cannot_widen_the_rail_past_the_agents_cap() {
    let mut workflow = diamond("nightly");
    workflow.name = "an extremely long workflow name ".repeat(8);
    let (_home, app) = app_with(&[workflow], &[]);

    assert_eq!(app.workflow_sidebar_width(200), 38);
}

#[test]
fn the_graph_is_drawn_as_marked_nodes_joined_by_wires() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    let screen = render(&mut app);

    assert!(screen.contains("▶ Start"), "the trigger node is missing");
    assert!(screen.contains("◆ Check"), "the condition node is missing");
    assert!(
        screen.contains("Start · trigger") && screen.contains("wheel/Page scroll"),
        "the selected step's persistent detail pane is missing: {screen}"
    );
    // One row per node, wires attaching on that same row: a node and the node
    // it feeds share a line. This is what buys the canvas the four-fold density
    // that drawing each node as a box cost.
    let row = screen
        .lines()
        .find(|line| line.contains("▶ Start"))
        .expect("the trigger is on some row");
    assert!(
        row.contains("◆ Check"),
        "a node sits inline with the node it feeds: {row}"
    );
    assert!(row.contains('─'), "joined by a wire on that row: {row}");
    assert!(screen.contains('▶'), "edges end in an arrowhead");
    assert!(
        screen.contains('│'),
        "a branch routes vertically between lanes"
    );
}

#[test]
fn a_branchs_port_names_are_drawn_on_the_wires_that_carry_them() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    let screen = render(&mut app);

    assert!(
        screen.contains("true") && screen.contains("false"),
        "which arm an edge is cannot be read off the picture otherwise: {screen}"
    );
}

#[test]
fn an_empty_store_says_so_and_points_at_the_copilot() {
    let (_home, mut app) = app_with(&[], &[]);

    let screen = render(&mut app);

    assert!(screen.contains("No workflows installed."));
    assert!(screen.contains("copilot"), "{screen}");
}

#[test]
fn the_selected_workflows_runs_are_listed_under_it_in_the_rail() {
    let run = RunRecord {
        id: "run-abc".into(),
        workflow_id: "nightly".into(),
        status: RunStatus::Succeeded,
        started_at: 1,
        finished_at: Some(2),
        steps: vec![RunStep {
            node_id: "start".into(),
            status: "success".into(),
            duration_ms: 3,
            input: None,
            output: None,
            diagnostics: Vec::new(),
            transcript: Vec::new(),
        }],
        pending_approvals: Vec::new(),
        error: None,
        inputs: Default::default(),
        trigger: None,
        origin: None,
        executor: None,
        cancel_requested: false,
        summary: None,
        diagnosis: None,
    };
    let (_home, mut app) = app_with(&[diamond("nightly")], &[run]);

    let screen = render(&mut app);

    assert!(screen.contains("succeeded"), "{screen}");
}

#[test]
fn selecting_a_run_overlays_it_on_the_graph() {
    let run = RunRecord {
        id: "run-abc".into(),
        workflow_id: "nightly".into(),
        status: RunStatus::Failed,
        started_at: 1,
        finished_at: Some(2),
        steps: vec![
            RunStep {
                node_id: "start".into(),
                status: "success".into(),
                duration_ms: 3,
                input: None,
                output: Some(json!({ "started": true })),
                diagnostics: Vec::new(),
                transcript: Vec::new(),
            },
            RunStep {
                node_id: "check".into(),
                status: "failed".into(),
                duration_ms: 4,
                input: None,
                output: None,
                diagnostics: Vec::new(),
                transcript: Vec::new(),
            },
        ],
        pending_approvals: Vec::new(),
        error: Some("boom".into()),
        inputs: Default::default(),
        trigger: None,
        origin: None,
        executor: None,
        cancel_requested: false,
        summary: None,
        diagnosis: None,
    };
    let (_home, mut app) = app_with(&[diamond("nightly")], &[run]);

    app.move_workflow_rail(false);
    let screen = render(&mut app);

    assert!(screen.contains("run "), "the title names the overlaid run");
    assert!(screen.contains('✓'), "a step that passed is marked");
    assert!(screen.contains('✗'), "the step that failed is marked");
    assert!(
        screen.contains("\"started\": true"),
        "the selected step's recorded result is shown: {screen}"
    );
    assert!(
        screen.contains("[f] Fix this run via agent"),
        "a failed run offers the existing repair action: {screen}"
    );
}

mod interaction_tests;
mod layout_tests;
mod outline_tests;
