//! Workflow pane interaction, resilience, and folded-layout rendering tests.

use super::*;

#[test]
fn the_inspector_is_a_view_of_its_own_that_i_opens_and_closes() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    // Closed, it is not on screen at all — the graph has the pane, with the
    // step under the cursor peeked at beneath it.
    let closed = render(&mut app);
    assert!(!closed.contains("i back to the graph"), "{closed}");

    app.wf.inspector_open = true;
    let open = render(&mut app);
    assert!(
        open.contains("manual"),
        "the open inspector shows the node's declaration: {open}"
    );
    assert!(open.contains("i back to the graph"), "{open}");
}

#[test]
fn the_inspector_follows_the_canvas_cursor() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    app.wf.inspector_open = true;

    app.move_graph_cursor(Move::Forward);
    let screen = render(&mut app);

    assert!(screen.contains("Check · condition"), "{screen}");
    assert!(screen.contains("=.ok"));
}

#[test]
fn the_copilot_pane_invites_an_instruction_before_the_first_turn() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    app.wf.focus = WorkflowFocus::Copilot;

    let screen = render(&mut app);

    assert!(screen.contains("add a Slack step"), "{screen}");
}

#[test]
fn the_sidebar_advertises_the_key_that_reaches_the_copilot() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    app.wf.focus = WorkflowFocus::Canvas;

    let screen = render(&mut app);

    // The copilot is no longer a column that can point at itself, so the
    // binding has to be named where the operator is. `c` is it (see
    // `keys/workflows.rs`); Tab is deliberately unbound in this tab and would
    // leave it entirely, so nothing may suggest it.
    assert!(screen.contains("c copilot"), "{screen}");
    assert!(!screen.contains("Tab to type"), "{screen}");
}

#[test]
fn a_copilot_thread_is_drawn_with_a_marker_per_kind_of_line() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    app.wf.focus = WorkflowFocus::Copilot;
    app.wf.draft = crate::ui::composer::insert_at("", 0, "add a step");
    app.submit_copilot().expect("turn");
    app.copilot_status("nightly", "applying ops".into());
    app.copilot_finished("nightly", "done".into(), vec!["+ node notify".into()], None);

    let screen = render(&mut app);

    assert!(screen.contains("❯ add a step"), "the instruction: {screen}");
    assert!(screen.contains("± + node notify"), "the change: {screen}");
    assert!(screen.contains("⏺ done"), "the reply: {screen}");
}

#[test]
fn the_focused_pane_is_the_one_with_the_highlighted_border() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    app.wf.focus = WorkflowFocus::Copilot;
    let screen = render(&mut app);

    // The composer's caret only renders in the focused pane, which is the
    // visible consequence of focus this test can read.
    assert!(screen.contains("⏎ send"), "{screen}");
}

#[test]
fn the_footer_teaches_this_tabs_bindings_rather_than_the_agents_tabs() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    let screen = render(&mut app);

    assert!(
        screen.contains("c copilot"),
        "the copilot is reached with c, not Tab: {screen}"
    );
    assert!(screen.contains("i inspect"), "{screen}");
    assert!(
        !screen.contains("⌥A answer"),
        "the Sessions-tab steering keys do nothing here"
    );
}

#[test]
fn tab_leaves_the_view_rather_than_cycling_panes_inside_it() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    let workflows = app.tab_index;

    app.on_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

    assert_ne!(
        app.tab_index, workflows,
        "Tab belongs to the top-level view ring; a tab inside a tab would trap it"
    );
}

#[test]
fn enter_opens_the_graph_and_esc_unwinds_one_level_at_a_time() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);
    let press = |app: &mut App, code| {
        app.on_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    };
    assert_eq!(app.wf_focus(), WorkflowFocus::Sidebar);

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.wf_focus(), WorkflowFocus::Canvas);

    press(&mut app, KeyCode::Char('c'));
    assert_eq!(app.wf_focus(), WorkflowFocus::Copilot);

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.wf_focus(), WorkflowFocus::Canvas);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.wf_focus(), WorkflowFocus::Sidebar);
}

#[test]
fn a_digit_jumps_straight_to_that_workflow_and_opens_it() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (_home, mut app) = app_with(&[diamond("first"), diamond("second")], &[]);

    app.on_event(Event::Key(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));

    assert_eq!(
        app.workflow_summaries()[app.selected_workflow_index()].id,
        "second"
    );
    assert_eq!(
        app.wf_focus(),
        WorkflowFocus::Canvas,
        "a digit opens the page it jumps to, as it does on every other nav"
    );
}

#[test]
fn a_stray_letter_in_the_sidebar_does_not_fire_a_content_action() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    app.on_event(Event::Key(KeyEvent::new(
        KeyCode::Char('i'),
        KeyModifiers::NONE,
    )));

    assert!(
        !app.wf_inspector_open(),
        "the inspector belongs to the canvas; the menu swallows its letters"
    );
}

#[test]
fn every_view_draws_at_any_terminal_width() {
    // What the old three-column layout could not do: on an 80-wide terminal it
    // left the canvas about twelve columns, less than one node box.
    for (width, height) in [(80, 24), (120, 30), (200, 50)] {
        let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

        let graph = render_sized(&mut app, width, height);
        assert!(graph.contains("▶ Start"), "{width}x{height}: {graph}");

        app.wf.inspector_open = true;
        assert!(render_sized(&mut app, width, height).contains("i back to the graph"));

        app.wf.focus = WorkflowFocus::Copilot;
        assert!(render_sized(&mut app, width, height).contains("Copilot"));
    }
}

#[test]
fn a_terminal_too_small_to_draw_anything_does_not_panic() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    render_sized(&mut app, 20, 6);
}

/// A trigger into a loop whose body is a four-step chain that returns to it —
/// the shape every "keep going until it is done" workflow has.
fn looping(id: &str) -> WorkflowRecord {
    WorkflowRecord {
        id: id.to_string(),
        name: format!("{id} loop"),
        description: String::new(),
        enabled: true,
        defaults: Default::default(),
        graph: serde_json::from_value(json!({
            "name": id,
            "nodes": [
                { "id": "start", "kind": "trigger", "name": "Start",
                  "config": { "trigger_kind": "manual" } },
                { "id": "watch", "kind": "loop", "name": "Until it settles",
                  "config": { "max_iterations": 8 } },
                { "id": "read", "kind": "tool_call", "name": "Read the state",
                  "config": { "slug": "medulla:shell" } },
                { "id": "fix", "kind": "agent", "name": "Fix it",
                  "config": { "prompt": "go" } },
                { "id": "again", "kind": "transform", "name": "Round again",
                  "config": { "expression": "=." } },
                { "id": "report", "kind": "transform", "name": "Report",
                  "config": { "expression": "=." } },
            ],
            "edges": [
                { "from_node": "start", "to_node": "watch" },
                { "from_node": "watch", "from_port": "body", "to_node": "read" },
                { "from_node": "watch", "from_port": "done", "to_node": "report" },
                { "from_node": "read", "to_node": "fix" },
                { "from_node": "fix", "to_node": "again" },
                // The arm that closes the loop.
                { "from_node": "again", "to_node": "watch" },
            ],
        }))
        .expect("graph parses"),
        source_path: None,
    }
}

#[test]
fn a_loop_folds_to_its_own_depth_rather_than_one_band_per_step() {
    let (_home, mut app) = app_with(&[looping("babysit")], &[]);
    render(&mut app);

    // Five columns of plan — start, the loop, and its three body steps, with
    // `Report` sharing the loop's own next column. The closing arm is not a
    // sixth step and must not rank as one.
    assert_eq!(app.workflow_layout().layers, 5);
    assert!(
        app.folded_rows() <= 6,
        "a six-node loop folded onto {} rows: {:?}",
        app.folded_rows(),
        app.band_heights()
    );
}

#[test]
fn a_node_name_is_never_broken_up_by_a_wire_crossing_it() {
    let (_home, mut app) = app_with(&[looping("babysit")], &[]);

    let screen = render(&mut app);

    // The loop's closing arm crosses this name's row on its way back. Its own
    // spaces used to resolve as the wire's box-drawing character.
    assert!(
        screen.contains("↺ Until it settles"),
        "the name is broken up: {screen}"
    );
    assert!(
        !screen.contains("↺─Until"),
        "a wire was drawn through the label: {screen}"
    );
}

#[test]
fn a_loops_closing_arm_comes_back_into_the_step_it_repeats() {
    let (_home, mut app) = app_with(&[looping("babysit")], &[]);

    let screen = render(&mut app);
    let row = screen
        .lines()
        .find(|line| line.contains("↺ Until it settles"))
        .expect("the loop is on some row");

    // Into the trailing edge — the side the work left by — rather than crossing
    // the whole box to arrive at its front, which is what routing the return
    // arm like a forward edge did.
    assert!(
        row.contains("↺ Until it settles ◀"),
        "the return arm does not arrive at the trailing edge: {row}"
    );
}

#[test]
fn a_fold_turns_against_the_frame_rather_than_costing_a_column() {
    let (_home, mut app) = app_with(&[looping("babysit")], &[]);

    // Narrow enough that the last column of a band ends flush against the panel
    // border, which is where the wire leaving it has nowhere to turn.
    let screen = render_sized(&mut app, 74, 30);

    // The width goes on the plan: reserving a right margin for that turn cost
    // roughly a whole layer per band, and a graph that folds after every step
    // is harder to read than one whose last fold turns against the frame.
    assert!(
        app.layers_per_band() > 1,
        "a margin ate the second column: {} layers per band",
        app.layers_per_band()
    );
    // And the turn is still drawn, rather than being clipped off the canvas.
    assert!(
        screen.lines().any(|line| line.contains('╮')),
        "the fold has no visible turn: {screen}"
    );
}

#[test]
fn a_port_name_in_the_gutter_leaves_the_arrowhead_its_cell() {
    let (_home, mut app) = app_with(&[diamond("nightly")], &[]);

    let screen = render(&mut app);

    // `false` is the longest port name in ordinary use and exactly fills the
    // gutter. Written a cell wider it lands on the arrowhead — and since the
    // label is painted after the arrow, it simply replaces it, leaving an arm
    // that reads as a wire which never arrived.
    assert!(
        screen.contains("false▶"),
        "the false arm lost its arrowhead: {screen}"
    );
    assert!(screen.contains("true"), "and the true arm is still named");
}

#[test]
fn a_folded_arms_port_name_stands_clear_of_the_wire() {
    let (_home, mut app) = app_with(&[looping("babysit")], &[]);

    // Narrow enough that the loop's body arm folds onto the next band.
    let screen = render_sized(&mut app, 74, 30);
    let row = screen
        .lines()
        .find(|line| line.contains("body"))
        .expect("the body arm is named somewhere");
    let at = row.find("body").expect("just found it");

    // A blank either side, so the name reads as a caption on the wire rather
    // than as part of the drawing: `╭body────────` ran the corner, the name and
    // the run together into one token.
    assert_eq!(&row[at - 1..at], " ", "no blank before the name: {row}");
    assert_eq!(&row[at + 4..at + 5], " ", "no blank after the name: {row}");
}
