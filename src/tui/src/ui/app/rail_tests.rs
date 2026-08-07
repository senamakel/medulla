//! Tests for the Agents rail's cursor identity: that the row the operator
//! picked stays picked while the rail grows, reorders, and loses rows
//! underneath them.
//!
//! The rail is rebuilt from live state every frame, so these pin the one
//! property that makes a positional cursor safe — that it is not actually
//! positional. The case that motivated them: the orchestrator spawns an agent,
//! its lane lands above the operator's own harness rows, and every row below
//! shifts by one. With an offset-only cursor the selection left the harness the
//! operator was attached to, which released the keyboard, restored the composer
//! and work panel, and resized the pane out from under them.

use medulla::protocol::HarnessProvider;
use medulla::ui::agents::{AgentLane, AgentRole, AgentRow, TaskState, TaskStatus};

use super::rail::{rail_anchor, resolve_rail_cursor, RailAnchor, RailRow};
use crate::worker::pty::{HarnessControl, PtyState, SessionRow};

/// A lane with `key`, carrying `tasks`.
fn lane(key: &str, tasks: Vec<TaskState>) -> AgentLane {
    AgentLane {
        key: key.into(),
        label: key.into(),
        role: AgentRole::Agent,
        turns: Vec::new(),
        last_at: 0,
        tasks,
        context_tokens: None,
        usage: Default::default(),
        harness_label: None,
        agent_id: None,
        session_id: None,
        parent_agent_id: None,
        descriptor: None,
        active_tasks: 0,
        work: None,
    }
}

fn task(id: &str) -> TaskState {
    TaskState {
        task_id: id.into(),
        status: TaskStatus::Running,
        turns: 1,
        last_at: 0,
        turn_blocks: Vec::new(),
        attention: None,
        question_id: None,
        work: None,
    }
}

/// An operator-started harness row with local id `id`.
fn harness(id: &str) -> SessionRow {
    SessionRow {
        id: id.into(),
        label: "local".into(),
        provider: HarnessProvider::Codex,
        state: PtyState::Running,
        cwd: "/workspace".into(),
        branch: None,
        launch_root: None,
        launch_commit: None,
        launch_checkout_identity: None,
        session_id: None,
        thread_name: None,
        started_at: 1,
        last_output_at: 1,
        last_error: None,
        busy: false,
        control: HarnessControl::User,
        user_spawned: true,
        attention: None,
    }
}

/// The rail as it stands before the orchestrator spawns anything: the
/// orchestrator lane, the action row, then the operator's own harness.
fn before() -> (Vec<AgentLane>, Vec<RailRow>) {
    let lanes = vec![lane("orchestrator", Vec::new())];
    let rows = vec![
        RailRow::Agent(AgentRow::Lane { lane_index: 0 }),
        RailRow::NewHarness,
        RailRow::HarnessSeparator,
        RailRow::Harness(harness("w_1")),
    ];
    (lanes, rows)
}

/// The same rail one spawn later: a new agent lane and its task sublane sit
/// between the action row and the operator's harnesses.
fn after() -> (Vec<AgentLane>, Vec<RailRow>) {
    let lanes = vec![
        lane("orchestrator", Vec::new()),
        lane("agent:builder", vec![task("task-1")]),
    ];
    let rows = vec![
        RailRow::Agent(AgentRow::Lane { lane_index: 0 }),
        RailRow::NewHarness,
        RailRow::Agent(AgentRow::Lane { lane_index: 1 }),
        RailRow::Agent(AgentRow::Sub {
            lane_index: 1,
            task: task("task-1"),
            last: true,
        }),
        RailRow::HarnessSeparator,
        RailRow::Harness(harness("w_1")),
    ];
    (lanes, rows)
}

#[test]
fn a_spawned_agent_does_not_move_the_cursor_off_the_operators_harness() {
    let (lanes, rows) = before();
    let anchor = rail_anchor(&rows[3], &lanes).expect("a harness row is selectable");
    assert_eq!(anchor, RailAnchor::Harness("w_1".into()));

    let (lanes, rows) = after();
    let cursor = resolve_rail_cursor(&rows, &lanes, Some(&anchor), 3);

    assert_eq!(
        cursor, 5,
        "the cursor must follow the harness row, not the offset it used to sit at"
    );
    assert!(
        matches!(&rows[cursor], RailRow::Harness(row) if row.id == "w_1"),
        "the resolved row must be the same harness"
    );
}

#[test]
fn an_anchored_lane_survives_lanes_appearing_above_it() {
    // The bug is not specific to harnesses: any row below an insertion point
    // moves. Here the operator is on a lane and a second lane is inserted
    // before it.
    let lanes = vec![lane("agent:builder", Vec::new())];
    let rows = [RailRow::Agent(AgentRow::Lane { lane_index: 0 })];
    let anchor = rail_anchor(&rows[0], &lanes).expect("a lane row is selectable");

    let lanes = vec![
        lane("agent:scout", Vec::new()),
        lane("agent:builder", Vec::new()),
    ];
    let rows = vec![
        RailRow::Agent(AgentRow::Lane { lane_index: 0 }),
        RailRow::Agent(AgentRow::Lane { lane_index: 1 }),
    ];

    assert_eq!(resolve_rail_cursor(&rows, &lanes, Some(&anchor), 0), 1);
}

#[test]
fn a_task_sublane_is_anchored_to_its_task_not_its_position() {
    // Sublanes are ordered running-first then most-recent, so they reorder on
    // their own without anything being spawned at all.
    let lanes = vec![lane("agent:builder", vec![task("task-a"), task("task-b")])];
    let rows = [
        RailRow::Agent(AgentRow::Lane { lane_index: 0 }),
        RailRow::Agent(AgentRow::Sub {
            lane_index: 0,
            task: task("task-a"),
            last: false,
        }),
        RailRow::Agent(AgentRow::Sub {
            lane_index: 0,
            task: task("task-b"),
            last: true,
        }),
    ];
    let anchor = rail_anchor(&rows[2], &lanes).expect("a sublane is selectable");
    assert_eq!(
        anchor,
        RailAnchor::Task {
            lane: "agent:builder".into(),
            task_id: "task-b".into(),
        }
    );

    // `task-b` overtakes `task-a`.
    let reordered = [
        RailRow::Agent(AgentRow::Lane { lane_index: 0 }),
        RailRow::Agent(AgentRow::Sub {
            lane_index: 0,
            task: task("task-b"),
            last: false,
        }),
        RailRow::Agent(AgentRow::Sub {
            lane_index: 0,
            task: task("task-a"),
            last: true,
        }),
    ];

    assert_eq!(resolve_rail_cursor(&reordered, &lanes, Some(&anchor), 2), 1);
}

#[test]
fn a_row_that_is_gone_falls_back_to_the_last_offset() {
    // A harness that exits and is forgotten takes its anchor with it. There is
    // no better answer then than where the cursor was, clamped into range —
    // and the caller re-anchors from whatever that lands on.
    let anchor = RailAnchor::Harness("w_gone".into());
    let (lanes, rows) = after();

    assert_eq!(resolve_rail_cursor(&rows, &lanes, Some(&anchor), 3), 3);
    assert_eq!(
        resolve_rail_cursor(&rows, &lanes, Some(&anchor), 99),
        rows.len() - 1,
        "an out-of-range fallback must clamp rather than index past the end"
    );
}

#[test]
fn dividers_are_not_anchorable() {
    let (lanes, rows) = before();

    assert_eq!(rail_anchor(&rows[2], &lanes), None);
    assert_eq!(rail_anchor(&rows[1], &lanes), Some(RailAnchor::NewHarness));
}

#[test]
fn an_empty_rail_resolves_to_zero() {
    let anchor = RailAnchor::NewHarness;

    assert_eq!(resolve_rail_cursor(&[], &[], Some(&anchor), 7), 0);
}

/// The whole failure, end to end, against a real harness on a real pty.
///
/// Unix-only: it needs a genuine pty client to occupy a harness row, and
/// `/bin/sh` is the portable stand-in the pty layer's own tests use.
#[cfg(unix)]
mod attached {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use medulla::config::LoadedConfig;
    use medulla::protocol::HarnessProvider;
    use medulla::runtime::mock::MockRuntime;
    use medulla::runtime::Runtime;
    use medulla::ui::events::{EventEnvelope, TuiEvent};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::ui::app::rail::RailRow;
    use crate::ui::app::App;
    use crate::ui::harness_pane::{HarnessFocus, LocalHarnesses};
    use crate::worker::pty::{HarnessControl, LaunchSpec, PtyManager};

    /// A harness that just sits there: a real child on a real pty, reading.
    fn spec() -> LaunchSpec {
        let mut env = HashMap::new();
        if let Ok(path) = std::env::var("PATH") {
            env.insert("PATH".to_string(), path);
        }
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        LaunchSpec {
            provider: HarnessProvider::Codex,
            bin: "/bin/sh".to_string(),
            cwd: "/".to_string(),
            env,
            extra_args: vec!["-c".to_string(), "read line".to_string()],
            skip_permissions: false,
            label: "test".to_string(),
            session_id: None,
            model: None,
            // The operator's own: what puts it in the rail's harness group.
            control: HarnessControl::User,
            user_spawned: true,
        }
    }

    /// [`LocalHarnesses`] over `sessions`, with an inert runtime — nothing here
    /// dispatches a task, so task resolution never runs.
    fn harnesses(sessions: PtyManager) -> LocalHarnesses {
        let config = medulla::daemon::DaemonConfig {
            providers: vec![HarnessProvider::Codex],
            default_provider: HarnessProvider::Codex,
            workspace: "/".to_string(),
            accessible_dirs: Vec::new(),
            env: HashMap::new(),
            task_timeout_ms: 1_000,
            capability_timeout_ms: None,
            concurrency: 1,
            status_throttle_ms: 1_000,
            max_pending: 1,
            model: None,
            agent: None,
            extra_args: Vec::new(),
            skip_permissions: false,
            router: None,
            custom_harnesses: Vec::new(),
            budget: None,
            attribution: true,
        };
        let run_task: medulla::daemon::providers::RunTaskFn =
            Arc::new(|_| Box::pin(async { Err("not used in these tests".to_string()) }));
        let send: medulla::daemon::SendFn = Arc::new(|_, _| {
            Box::pin(async {}) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        });
        LocalHarnesses {
            sessions,
            runtimes: Arc::new(Mutex::new(vec![medulla::daemon::DaemonRuntime::new(
                config, run_task, send,
            )])),
            hub_address: "medulla-orchestrator".to_string(),
            env: HashMap::new(),
            workspace: "/".to_string(),
            providers: vec![HarnessProvider::Codex],
            custom_harnesses: Vec::new(),
            router: None,
            attribution: true,
        }
    }

    fn draw(app: &mut App) {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal.draw(|f| app.draw(f)).expect("draw");
    }

    #[test]
    fn spawning_an_agent_does_not_evict_the_operator_from_the_harness_they_are_in() {
        let sessions = PtyManager::new();
        let id = sessions.open(spec()).expect("a pty");
        let mut app = App::new(
            Arc::new(MockRuntime::demo()) as Arc<dyn Runtime>,
            LoadedConfig::defaults("medulla.tui.json".into()),
        );
        app.harnesses = Some(harnesses(sessions));
        app.tab_index = crate::ui::app::TABS
            .iter()
            .position(|t| *t == "Agents")
            .expect("the Agents tab");

        // The operator arrows onto their harness and takes the keyboard.
        let rows = app.rail_rows();
        let index = rows
            .iter()
            .position(|row| row.session_id() == Some(id.as_str()))
            .expect("the harness has a rail row");
        app.set_rail_cursor(index);
        app.harness_focus = HarnessFocus::Attached(id.clone());
        draw(&mut app);
        assert_eq!(
            app.attached_harness(),
            Some(id.as_str()),
            "precondition: the operator is typing into the harness"
        );

        // The orchestrator spawns an agent. Its lane — and its task sublane —
        // land above the harness group, moving every row below them down.
        app.snapshot.events.push(EventEnvelope {
            seq: 9_000,
            at: 9_000,
            event: TuiEvent::TaskStart {
                task_id: "task-spawned".into(),
                instruction: "Audit the rail".into(),
                depth: 2,
                agent_id: Some("dev-2".into()),
                contract: None,
            },
        });
        let moved = app.rail_rows();
        let now_at = moved
            .iter()
            .position(|row| row.session_id() == Some(id.as_str()))
            .expect("the harness still has a rail row");
        assert!(
            now_at > index,
            "precondition: the spawn must have pushed the harness row down"
        );

        draw(&mut app);

        assert_eq!(
            app.attached_harness(),
            Some(id.as_str()),
            "an agent starting elsewhere must not take the keyboard out of the harness"
        );
        let rows = app.rail_rows();
        assert!(
            matches!(rows.get(app.agent_index()), Some(RailRow::Harness(row)) if row.id == id),
            "the cursor must still be on the harness, not on the row that slid into its offset"
        );

        app.harnesses
            .as_ref()
            .expect("harnesses")
            .sessions
            .shutdown();
    }
}
