//! Tests for the Hosts page and the `H` selection: the form, issuing a key,
//! removal, the switcher's rows and keys, and what a selection does to the
//! tabs that follow it.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use medulla::config::{LoadedConfig, RemoteHostSection};
use medulla::runtime::mock::MockRuntime;
use medulla::runtime::Runtime;

use super::super::types::{tab_pos, App, Cmd, PromptKind};
use super::{HostFormField, HostLiveness, HostSelection};

/// An app with `hosts` declared, a scratch home to pair into, and a writable
/// config file to persist to.
fn app_with_hosts(hosts: &[(&str, u16)]) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let rt: Arc<dyn Runtime> = Arc::new(MockRuntime::demo());
    let mut loaded = LoadedConfig::defaults(dir.path().join("config.toml"));
    loaded.config.remote_hosts = hosts
        .iter()
        .map(|(host, udp_port)| RemoteHostSection {
            host: host.to_string(),
            udp_port: *udp_port,
            enabled: true,
            ..RemoteHostSection::default()
        })
        .collect();
    let mut app = App::new(rt, loaded);
    app.set_config_path(dir.path().join("config.toml"));
    app.set_medulla_home(dir.path().join("home"));
    app.tab_index = tab_pos("Hosts");
    app.rebuild_host_entries();
    (dir, app)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn submit(app: &mut App, text: &str) -> Option<Cmd> {
    let prompt = app.prompt.take().expect("a prompt is open");
    match prompt.kind {
        PromptKind::HostField { form, field } => app.submit_host_field(*form, field, text.into()),
        other => panic!("unexpected prompt {other:?}"),
    }
}

#[test]
fn the_page_lists_configured_hosts_with_their_liveness() {
    let (_dir, app) = app_with_hosts(&[("gpu.example.com", 60001), ("tower.local", 0)]);
    assert_eq!(app.hosts.entries.len(), 2);
    assert_eq!(app.hosts.entries[0].liveness, HostLiveness::Idle);
    assert_eq!(app.hosts.entries[1].liveness, HostLiveness::Unpaired);
}

#[test]
fn adding_a_host_walks_the_form_persists_it_and_issues_a_key() {
    let (dir, mut app) = app_with_hosts(&[]);
    let clipboard = app.capture_clipboard();
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('a'))));
    assert!(matches!(
        app.prompt.as_ref().map(|p| &p.kind),
        Some(PromptKind::HostField { field: HostFormField::Name, .. })
    ));
    // Name is required: an empty answer re-asks.
    assert!(submit(&mut app, "").is_none());
    assert!(matches!(
        app.prompt.as_ref().map(|p| &p.kind),
        Some(PromptKind::HostField { field: HostFormField::Name, .. })
    ));
    submit(&mut app, "GPU Box");
    submit(&mut app, "gpu.example.com");
    // A bad port re-asks too.
    submit(&mut app, "sixty");
    assert!(matches!(
        app.prompt.as_ref().map(|p| &p.kind),
        Some(PromptKind::HostField { field: HostFormField::UdpPort, .. })
    ));
    submit(&mut app, "");
    submit(&mut app, "ops");
    submit(&mut app, "");
    submit(&mut app, "");
    submit(&mut app, "/srv/work");
    assert!(app.prompt.is_none(), "the form is done");

    // Written, with an explicit id and a port picked for it.
    let written = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(written.contains("[[remoteHosts]]"), "{written}");
    assert!(written.contains("id = \"gpu-box\""), "{written}");
    assert!(written.contains("udpPort = 60001"), "{written}");

    // And the key was minted, shown, and copied.
    let pending = app.hosts.pending_key.clone().expect("a key is pending");
    assert_eq!(pending.host_id, "gpu-box");
    assert!(pending.command.starts_with("medulla daemon HK1-"), "{}", pending.command);
    assert_eq!(clipboard.lock().unwrap().as_slice(), [pending.command.clone()]);
    let state_dir =
        crate::remote::client::task::client_state_dir(&dir.path().join("home"), "gpu-box");
    assert!(medulla_link::keys::node_path(&state_dir).exists());
    assert_eq!(app.hosts.entries[0].liveness, HostLiveness::Idle);
}

#[test]
fn re_issuing_replaces_the_key_and_removal_forgets_everything() {
    let (dir, mut app) = app_with_hosts(&[("gpu.example.com", 60001)]);
    let _ = app.capture_clipboard();
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('i'))));
    let first = app.hosts.pending_key.clone().unwrap().command;
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('i'))));
    let second = app.hosts.pending_key.clone().unwrap().command;
    assert_ne!(first, second, "a re-issued key is a new key");

    let host_id = app.hosts.entries[0].id.clone();
    let state_dir =
        crate::remote::client::task::client_state_dir(&dir.path().join("home"), &host_id);
    assert!(state_dir.exists());

    // `d` arms; anything but `y` cancels; `y` removes.
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('d'))));
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Esc)));
    assert_eq!(app.hosts.entries.len(), 1, "cancelled");
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('d'))));
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Char('y'))));
    assert!(app.hosts.entries.is_empty());
    assert!(app.hosts.pending_key.is_none());
    assert!(!state_dir.exists(), "the pairing is forgotten");
    let written = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(!written.contains("gpu.example.com"), "{written}");
}

#[test]
fn the_switcher_lists_this_device_all_hosts_and_each_host() {
    let (_dir, mut app) = app_with_hosts(&[("gpu.example.com", 60001), ("tower.local", 0)]);
    // No local sessions: "This device" is not offered, since there is nothing
    // to start here.
    let rows = app.switcher_rows();
    assert_eq!(rows[0].0, HostSelection::All);
    assert_eq!(rows[1].0, HostSelection::Host("gpu-example-com".to_string()));
    assert_eq!(rows[2].0, HostSelection::Host("tower-local".to_string()));
    assert_eq!(rows[2].2, HostLiveness::Unpaired);

    // `H` from another tab opens it; `H` again cycles; Enter picks.
    app.tab_index = tab_pos("Feedback");
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::SHIFT,
    )));
    assert!(app.host_switcher.is_some());
    assert_eq!(app.host_switcher.as_ref().unwrap().index, 0, "on the current selection");
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::SHIFT,
    )));
    let cmd = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Enter)));
    assert!(app.host_switcher.is_none());
    assert_eq!(*app.current_host(), HostSelection::Host("gpu-example-com".to_string()));
    assert!(
        matches!(cmd, Some(Cmd::ConnectRemoteHost { ref host_id }) if host_id == "gpu-example-com"),
        "selecting a paired host dials it: {cmd:?}"
    );
    assert_eq!(app.current_host_label(), "gpu.example.com");

    // Escape closes without changing anything.
    app.open_host_switcher();
    let _ = app.on_event(crate::ui::events::TuiEventInput::Key(key(KeyCode::Esc)));
    assert!(app.host_switcher.is_none());
    assert_eq!(*app.current_host(), HostSelection::Host("gpu-example-com".to_string()));
}

#[test]
fn selecting_an_unpaired_host_does_not_dial_it() {
    let (_dir, mut app) = app_with_hosts(&[("tower.local", 0)]);
    let cmd = app.set_current_host(HostSelection::Host("tower-local".to_string()));
    assert!(cmd.is_none());
    assert!(app.status.contains("not paired"), "{}", app.status);
}

#[test]
fn a_selection_scopes_which_sessions_the_rail_shows() {
    let (_dir, mut app) = app_with_hosts(&[("gpu.example.com", 60001), ("tower.local", 60002)]);
    let row = |id: &str| medulla::protocol::RemoteSessionRow {
        id: id.to_string(),
        label: id.to_string(),
        provider: "shell".to_string(),
        preset: None,
        state: medulla::protocol::RemoteSessionState::Running,
        cwd: "/".to_string(),
        name: None,
        thread_name: None,
        started_at: 1,
        last_output_at: 1,
        busy: false,
        working: false,
        attention: None,
        bracketed_paste: false,
        last_error: None,
    };
    app.remote_host_connected("gpu-example-com", Default::default());
    app.remote_host_sessions("gpu-example-com", vec![row("w_1")]);
    app.remote_host_connected("tower-local", Default::default());
    app.remote_host_sessions("tower-local", vec![row("w_2")]);

    let ids = |app: &App| -> Vec<String> {
        app.own_session_rows_for_test()
            .into_iter()
            .map(|row| row.id)
            .collect()
    };
    assert_eq!(ids(&app).len(), 2, "the combined view lists both");
    app.set_current_host(HostSelection::Host("tower-local".to_string()));
    assert_eq!(ids(&app), vec!["tower-local::w_2".to_string()]);
    app.set_current_host(HostSelection::ThisDevice);
    assert!(ids(&app).is_empty(), "this device hosts nothing");
    app.set_current_host(HostSelection::All);
    assert_eq!(ids(&app).len(), 2);
}
