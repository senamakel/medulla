use super::rows::{compare, visible_rows};
use super::*;
use crate::ui::app::resources::types::ResourcesState;

fn process(pid: u32, command: &str, cpu: f64, mem: u64) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        ppid: None,
        exe: Some(command.into()),
        cmd: vec![command.into()],
        cwd: Some(format!("/workspace/{command}")),
        start_time: 100,
        owner_uid: None,
        cpu_percent: cpu,
        memory_bytes: mem,
        disk_read_bytes: 0,
        disk_written_bytes: 0,
        threads: pid as usize,
    }
}
fn snapshot() -> ResourceSnapshot {
    ResourceSnapshot::default().with_processes(vec![
        process(10, "claude", 10.0, 4000),
        process(20, "codex-acp", 20.0, 3000),
        process(30, "openhuman", 30.0, 2000),
        process(40, "aider", 40.0, 1000),
    ])
}

#[test]
fn remote_agents_render_beneath_each_host_heading() {
    let row = |id: &str, kind: &str, pid| medulla::protocol::RemoteSessionRow {
        id: id.into(),
        label: kind.into(),
        provider: kind.to_lowercase(),
        preset: None,
        state: medulla::protocol::RemoteSessionState::Running,
        cwd: "/work".into(),
        name: None,
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_000_000,
        busy: true,
        working: true,
        attention: None,
        bracketed_paste: false,
        last_error: None,
        usage: Some(medulla::protocol::AgentUsage {
            kind: kind.into(),
            pid,
            cpu: 1.0,
            rss: 1024,
            threads: 2,
            started_at: 1_700_000_000,
            cpu_history: vec![0.5, 1.0],
        }),
    };
    let remote_hosts = vec![
        ("alpha".into(), vec![row("w_a", "Claude", 101)]),
        ("beta".into(), vec![row("w_b", "Codex", 202)]),
    ];
    let backend = ratatui::backend::TestBackend::new(120, 18);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut state = ResourcesState::default();
    terminal
        .draw(|frame| {
            draw_agents(
                frame,
                frame.area(),
                &ResourceSnapshot::default(),
                &remote_hosts,
                &mut state,
                &ResourcesTheme::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Remote",
        "alpha",
        "Remote{alpha}",
        "Claude",
        "beta",
        "Remote{beta}",
        "Codex",
    ] {
        assert!(
            rendered.contains(expected),
            "missing {expected:?}:\n{rendered}"
        );
    }
    assert_eq!(
        rendered.matches("alpha").count(),
        2,
        "alpha heading and row:\n{rendered}"
    );
    assert_eq!(
        rendered.matches("beta").count(),
        2,
        "beta heading and row:\n{rendered}"
    );
    assert!(rendered.find("alpha").unwrap() < rendered.find("beta").unwrap());
}

#[test]
fn remote_host_groups_preserve_local_agent_tree_order() {
    let mut parent = process(100, "claude", 1.0, 100);
    parent.ppid = None;
    let mut child = process(50, "codex-acp", 2.0, 200);
    child.ppid = Some(100);
    let snapshot = ResourceSnapshot::default().with_processes(vec![parent, child]);
    let remote = |id: &str, kind: &str, pid| medulla::protocol::RemoteSessionRow {
        id: id.into(),
        label: kind.into(),
        provider: kind.to_lowercase(),
        preset: None,
        state: medulla::protocol::RemoteSessionState::Running,
        cwd: "/work".into(),
        name: None,
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_000_000,
        busy: false,
        working: false,
        attention: None,
        bracketed_paste: false,
        last_error: None,
        usage: Some(medulla::protocol::AgentUsage {
            kind: kind.into(),
            pid,
            cpu: pid as f64,
            rss: 1_024,
            threads: 1,
            started_at: 1_700_000_000,
            cpu_history: vec![],
        }),
    };
    // Deliberately pass hosts and rows out of display order. Remote hosts
    // group by host, but local tree order is kept exactly as discovered.
    let hosts = vec![
        ("beta".into(), vec![remote("b", "Beta", 1)]),
        (
            "alpha".into(),
            vec![remote("a2", "Alpha", 999), remote("a1", "Alpha", 2)],
        ),
    ];
    let state = ResourcesState {
        agent_tree: true,
        ..ResourcesState::default()
    };
    let rows = visible_rows_with_remote(&snapshot, &hosts, &state);
    assert_eq!(
        rows.iter()
            .map(|row| (row.host.as_deref(), row.pid))
            .collect::<Vec<_>>(),
        [
            (None, 100),
            (None, 50),
            (Some("alpha"), 2),
            (Some("alpha"), 999),
            (Some("beta"), 1)
        ],
    );
}

#[test]
fn remote_agents_share_regex_filter_semantics_and_have_host_scoped_identity() {
    let row = |id: &str, kind: &str, pid| medulla::protocol::RemoteSessionRow {
        id: id.into(),
        label: kind.into(),
        provider: kind.to_lowercase(),
        preset: None,
        state: medulla::protocol::RemoteSessionState::Running,
        cwd: "/work".into(),
        name: None,
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_000_000,
        busy: false,
        working: false,
        attention: None,
        bracketed_paste: false,
        last_error: None,
        usage: Some(medulla::protocol::AgentUsage {
            kind: kind.into(),
            pid,
            cpu: 1.0,
            rss: 1_024,
            threads: 2,
            started_at: 1_700_000_000,
            cpu_history: vec![],
        }),
    };
    let hosts = vec![
        ("alpha".into(), vec![row("w_same", "Claude", 55)]),
        ("beta".into(), vec![row("w_same", "Codex", 55)]),
    ];
    let state = ResourcesState {
        agent_filter: Some("!^cod".into()),
        ..ResourcesState::default()
    };
    let rows = visible_rows_with_remote(&ResourceSnapshot::default(), &hosts, &state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "Codex");
    assert_eq!(rows[0].host.as_deref(), Some("beta"));
    assert_eq!(rows[0].session_id.as_deref(), Some("beta/w_same"));
}

#[test]
fn classifies_known_and_external_agent_processes() {
    let rows = rows(&snapshot());
    assert_eq!(
        rows.iter().map(|row| row.kind.as_str()).collect::<Vec<_>>(),
        ["Claude", "Codex-ACP", "OpenHuman", "Aider"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.origin.as_str())
            .collect::<Vec<_>>(),
        ["external", "external", "external", "external"]
    );
}

#[test]
fn unregistered_bare_codex_process_is_named_codex_with_an_external_origin() {
    let rows =
        rows(&ResourceSnapshot::default().with_processes(vec![process(987_105, "codex", 0.0, 1)]));
    assert_eq!(rows.len(), 1, "the harness-named process must be visible");
    assert_eq!(rows[0].kind, "Codex");
    assert_eq!(rows[0].origin, "external");
}

#[test]
fn registry_classifies_session_task_and_host_origins() {
    let registry = medulla::resources::launch::agent_registry();
    let registrations = [
        medulla::resources::launch::AgentRegistration {
            pid: 987_101,
            started_at_secs: 100,
            task_id: None,
            session_id: Some("session-label".into()),
            provider: "claude".into(),
        },
        medulla::resources::launch::AgentRegistration {
            pid: 987_102,
            started_at_secs: 100,
            task_id: Some("task-7".into()),
            session_id: None,
            provider: "codex-acp".into(),
        },
        medulla::resources::launch::AgentRegistration {
            pid: 987_103,
            started_at_secs: 100,
            task_id: None,
            session_id: None,
            provider: "openhuman".into(),
        },
    ];
    let _guards: Vec<_> = registrations
        .iter()
        .cloned()
        .map(|registration| registry.register_at(registration))
        .collect();
    let snapshot = ResourceSnapshot::default().with_processes(vec![
        process(987_101, "claude", 1.0, 1),
        process(987_102, "codex-acp", 2.0, 2),
        process(987_103, "openhuman", 3.0, 3),
        process(987_104, "aider", 4.0, 4),
    ]);
    let rows = rows(&snapshot);
    assert_eq!(
        rows.iter()
            .map(|row| row.origin.as_str())
            .collect::<Vec<_>>(),
        ["session-label", "task task-7", "host", "external"]
    );
    assert_eq!(
        rows.iter().map(|row| row.kind.as_str()).collect::<Vec<_>>(),
        ["Claude", "Codex-ACP", "OpenHuman", "Aider"]
    );
}

#[test]
fn tree_mode_nests_child_processes_under_agent_rows() {
    let snapshot = ResourceSnapshot::default().with_processes(vec![
        process(10, "claude", 10.0, 4000),
        ProcessSnapshot {
            pid: 11,
            ppid: Some(10),
            exe: Some("node".into()),
            cmd: vec!["node".into()],
            cwd: Some("/work".into()),
            start_time: 101,
            owner_uid: None,
            cpu_percent: 2.0,
            memory_bytes: 512,
            disk_read_bytes: 0,
            disk_written_bytes: 0,
            threads: 1,
        },
    ]);
    let state = ResourcesState {
        agent_tree: true,
        ..ResourcesState::default()
    };
    let rows = visible_rows(&snapshot, &state);
    assert_eq!(
        rows.iter()
            .map(|row| (&*row.kind, row.depth))
            .collect::<Vec<_>>(),
        [("Claude", 0), ("Process", 1)]
    );
}

#[test]
fn sorts_each_column_reverses_and_filters_codex_by_regex() {
    let snap = snapshot();
    let mut state = ResourcesState::default();
    for sort in [
        AgentSort::Pid,
        AgentSort::Kind,
        AgentSort::Origin,
        AgentSort::Workspace,
        AgentSort::Threads,
        AgentSort::Memory,
        AgentSort::Cpu,
        AgentSort::Runtime,
    ] {
        state.agent_sort = sort;
        let sorted = visible_rows(&snap, &state);
        assert_eq!(sorted.len(), 4);
        assert!(sorted
            .windows(2)
            .all(|w| compare(&w[0], &w[1], sort) != Ordering::Greater));
    }
    state.agent_sort = AgentSort::Cpu;
    state.agent_reverse = true;
    assert_eq!(visible_rows(&snap, &state)[0].pid, 40);
    state.agent_reverse = false;
    state.agent_filter = Some("!^cod".into());
    assert_eq!(
        visible_rows(&snap, &state)
            .iter()
            .map(|r| r.kind.as_str())
            .collect::<Vec<_>>(),
        ["Codex-ACP"]
    );
}

/// The process table a Mac running Cursor, Claude Code and the Codex app
/// actually has: only the two harnesses are rows, each named for what it is,
/// and each carrying its whole tree's memory.
#[test]
fn only_harnesses_are_rows_and_each_is_named_for_what_it_is() {
    let at = |pid: u32, ppid: Option<u32>, exe: &str, cmd: &[&str], mem: u64| ProcessSnapshot {
        pid,
        ppid,
        exe: Some(exe.into()),
        cmd: cmd.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: Some("/work/repo".into()),
        start_time: 100,
        owner_uid: None,
        cpu_percent: 1.0,
        memory_bytes: mem,
        disk_read_bytes: 0,
        disk_written_bytes: 0,
        threads: 4,
    };
    let codex = "/Users/me/.codex/packages/app-server-daemon/bin/codex";
    let snapshot = ResourceSnapshot::default().with_processes(vec![
        at(687, Some(1), "/Applications/Cursor.app/Contents/MacOS/Cursor", &["Cursor"], 10),
        at(
            1847,
            Some(687),
            "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Renderer).app/Contents/MacOS/Cursor Helper (Renderer)",
            &["Cursor Helper (Renderer)", "--type=renderer"],
            10,
        ),
        at(9065, Some(1), "/Users/me/.medulla/bin/medulla", &["medulla"], 10),
        at(
            9235,
            Some(9065),
            "/Users/me/.local/share/claude/versions/2.1.310",
            &["claude", "--session-id", "abc"],
            500,
        ),
        at(9896, Some(9235), "/Users/me/.medulla/bin/medulla", &["medulla", "mcp"], 50),
        at(10027, Some(9235), "/usr/local/bin/chrome-devtools-mcp", &["chrome-devtools-mcp"], 50),
        at(49530, Some(1), codex, &["codex", "app-server", "daemon"], 10),
        at(41393, Some(49530), codex, &["codex", "app-server", "--listen"], 100),
    ]);
    let rows = rows(&snapshot);
    assert_eq!(
        rows.iter()
            .map(|row| (row.pid, row.kind.as_str(), row.memory, row.threads))
            .collect::<Vec<_>>(),
        [(9235, "Claude", 600, 12), (49530, "Codex Server", 110, 8)]
    );
    assert!(rows.iter().all(|row| row.workspace == "/work/repo"));
}
