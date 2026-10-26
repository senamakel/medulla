//! Selecting, opening, and following agent rows.

use super::*;

#[cfg(unix)]
#[test]
fn enter_on_a_registered_session_agent_selects_its_sessions_rail_row() {
    use medulla::protocol::HarnessProvider;

    let mut app = crate::ui::app::rail::tests::hosting_app();
    let harnesses = app.local_sessions().expect("test host").clone();
    let choice = harnesses
        .choices()
        .into_iter()
        .find(|choice| choice.provider == HarnessProvider::Codex)
        .expect("codex harness is configured");
    let session_id = harnesses
        .open_unmanaged_named(&choice, "/", false, Some("agents acceptance".into()))
        .expect("a /bin/sh session starts");

    let pid = 45_001;
    let started = 100;
    let _registration = medulla::resources::launch::agent_registry().register_at(
        medulla::resources::launch::AgentRegistration {
            pid,
            started_at_secs: started,
            task_id: None,
            session_id: Some(session_id.clone()),
            provider: "claude".into(),
        },
    );
    let mut process = test_agent_process(pid, "claude", 10.0);
    process.start_time = started;
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default().with_processes(vec![process]),
    ));
    app.resources.agent_selected = Some(AgentIdentity {
        host: None,
        pid,
        started,
    });

    assert!(app.resources_key(KeyCode::Enter));
    let sessions_tab = crate::ui::app::types::TABS
        .iter()
        .position(|tab| *tab == "Sessions")
        .unwrap();
    assert_eq!(app.tab_index, sessions_tab);
    let rows = app.rail_rows();
    let selected_row = rows
        .iter()
        .position(|row| row.session_id() == Some(session_id.as_str()))
        .expect("registered session is on the rail");
    assert_eq!(app.rail_cursor(), selected_row);
    harnesses.sessions.shutdown();
}

#[test]
fn remote_agent_rows_participate_in_selection_and_enter_opens_the_host_session() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let mut host = crate::ui::app::remote_hosts::RemoteHostState::default();
    host.rows.push(medulla::protocol::RemoteSessionRow {
        id: "w_remote".into(),
        label: "remote agent".into(),
        provider: "claude".into(),
        preset: None,
        state: medulla::protocol::RemoteSessionState::Running,
        cwd: "/remote/work".into(),
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
            kind: "Claude".into(),
            pid: 77,
            cpu: 2.0,
            rss: 1024,
            threads: 2,
            started_at: 1_700_000_000,
            cpu_history: vec![1.0, 2.0],
        }),
    });
    app.remote_hosts.insert("tower".into(), host);

    assert!(app.resources_key(KeyCode::Down));
    assert_eq!(
        app.resources.agent_selected,
        Some(AgentIdentity {
            host: Some("tower".into()),
            pid: 77,
            started: 1_700_000_000,
        })
    );
    assert!(app.resources_key(KeyCode::Enter));
    let sessions_tab = crate::ui::app::types::TABS
        .iter()
        .position(|tab| *tab == "Sessions")
        .unwrap();
    assert_eq!(app.tab_index, sessions_tab);
    let rail_rows = app.rail_rows();
    let selected_row = rail_rows
        .iter()
        .position(|row| row.session_id() == Some("tower/w_remote"))
        .expect("remote agent session is available on the Sessions rail");
    assert_eq!(app.rail_cursor(), selected_row);
}

#[test]
fn agent_follow_tracks_identity_through_sorting_and_snapshot_reordering() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let processes: Vec<_> = (100..120)
        .map(|pid| test_agent_process(pid, "claude", f64::from(pid)))
        .collect();
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(processes.clone()),
    ));
    app.resources.agent_selected = Some(AgentIdentity {
        host: None,
        pid: 119,
        started: 1,
    });
    assert!(app.resources_key(KeyCode::Char('F')));
    assert_eq!(
        app.resources.agent_follow,
        Some(AgentIdentity {
            host: None,
            pid: 119,
            started: 1
        })
    );

    let render = |app: &mut App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 80, 24)))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    let first = render(&mut app);
    assert_eq!(
        app.resources.agent_selected,
        Some(AgentIdentity {
            host: None,
            pid: 119,
            started: 1
        })
    );
    assert!(first.contains("┐Follow 119┌"), "{first}");
    assert!(
        first.contains("119"),
        "followed process should be visible: {first}"
    );

    assert!(app.resources_key(KeyCode::Right));
    assert!(app.resources_key(KeyCode::Right));
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(processes.into_iter().rev().collect()),
    ));
    let reordered = render(&mut app);
    assert_eq!(
        app.resources.agent_selected,
        Some(AgentIdentity {
            host: None,
            pid: 119,
            started: 1
        })
    );
    assert!(reordered.contains("┐Follow 119┌"));
    assert!(
        reordered.contains("119"),
        "followed process should stay visible: {reordered}"
    );
}

#[test]
fn the_highlighted_m_in_the_cpu_title_opens_the_menu() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    assert!(app.resources_key(KeyCode::Char('m')));
    assert!(app.resources.menu_open);
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    let text = screen_rows(&terminal).join("\n");
    assert!(text.contains("┐menu┌───"), "menu overlay missing: {text}");
    assert!(text.contains("1-4  show or hide a box"), "{text}");
    assert!(app.resources_key(KeyCode::Char('m')));
    assert!(!app.resources.menu_open);
}

#[test]
fn letters_typed_into_the_agent_filter_are_not_resources_shortcuts() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let resources_tab = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.tab_index = resources_tab;
    app.resources.agent_filter_editing = true;
    for letter in ['m', 'o', '?'] {
        assert!(app.resources_key(KeyCode::Char(letter)));
    }
    assert_eq!(app.resources.agent_filter.as_deref(), Some("mo?"));
    assert!(!app.resources.menu_open);
    assert!(!app.resources.help_open);
    assert_eq!(app.tab_index, resources_tab);
}
