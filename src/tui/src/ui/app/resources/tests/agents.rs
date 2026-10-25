//! Agent rows and the guarded process signals.

use super::*;

#[test]
fn agent_rows_render_with_classification_at_desktop_and_narrow_sizes() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default().with_processes(vec![
            test_agent_process(10, "claude", 12.0),
            test_agent_process(20, "codex-acp", 22.0),
            test_agent_process(30, "openhuman", 32.0),
            test_agent_process(40, "aider", 42.0),
        ]),
    ));
    for (width, height) in [(170, 42), (80, 24)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, width, height)))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for expected in [
            "Claude",
            "Codex-ACP",
            "OpenHuman",
            "External",
            "external",
            "Pid:",
        ] {
            assert!(
                text.contains(expected),
                "missing {expected} at {width}x{height}: {text}"
            );
        }
    }
}

static SIGNALS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static SIGNAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn test_process_owned(_pid: u32, _started: u64, uid: Option<&str>) -> bool {
    uid == Some("test-user")
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn never_owned(_pid: u32, _started: u64, _uid: Option<&str>) -> bool {
    false
}
fn test_signal(
    _pid: u32,
    _started: u64,
    _uid: Option<&str>,
    _signal: AgentSignal,
) -> std::io::Result<()> {
    SIGNALS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn agent_kill_waits_for_confirmation_and_uses_injected_signal_function() {
    let _guard = SIGNAL_TEST_LOCK.lock().unwrap();
    SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(42, "claude", 10.0)]),
    ));
    app.resources.agent_signal_fn = test_signal;
    app.resources.agent_process_owned_fn = test_process_owned;
    assert!(app.resources_key(KeyCode::Char('k')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(app.resources.agent_signal_armed.is_some());
    assert!(app.resources_key(KeyCode::Char('y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(app.resources.agent_signal_armed.is_some());
    assert!(app.resources_key(KeyCode::Char('Y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(app.resources.agent_signal_armed.is_none());
}

static HIDDEN_SIGNALS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn hidden_test_signal(
    _pid: u32,
    _started: u64,
    _uid: Option<&str>,
    _signal: AgentSignal,
) -> std::io::Result<()> {
    HIDDEN_SIGNALS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[test]
fn hidden_agents_box_cannot_arm_or_confirm_a_process_signal() {
    HIDDEN_SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(43, "claude", 10.0)]),
    ));
    app.resources.agent_signal_fn = hidden_test_signal;
    app.resources.shown.agents = false;

    assert!(!app.resources_key(KeyCode::Char('k')));
    assert!(app.resources.agent_signal_armed.is_none());

    app.resources.agent_signal_armed = Some(AgentSignalConfirmation {
        pid: 43,
        started: 1,
        signal: AgentSignal::Kill,
        stage: AgentSignalConfirmStage::First,
    });
    assert!(app.resources_key(KeyCode::Char('y')));
    assert!(app.resources.agent_signal_armed.is_none());
    assert_eq!(HIDDEN_SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn agent_signal_confirmation_stays_visible_in_detail_view() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let mut process = test_agent_process(44, "aider", 10.0);
    process.exe = Some("/usr/bin/aider".into());
    process.cmd = vec![
        "API_TOKEN=envsecret".into(),
        "aider".into(),
        "--api-key".into(),
        "supersecret".into(),
        "--workspace".into(),
        "/tmp/work".into(),
    ];
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default().with_processes(vec![process]),
    ));
    app.resources.agent_selected = Some(AgentIdentity {
        host: None,
        pid: 44,
        started: 1,
    });
    app.resources.agent_detail = true;
    app.resources.shown.cpu = false;
    app.resources.shown.mem = false;
    app.resources.shown.net = false;
    app.resources.agent_signal_armed = Some(AgentSignalConfirmation {
        pid: 44,
        started: 1,
        signal: AgentSignal::Kill,
        stage: AgentSignalConfirmStage::First,
    });

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 80, 24)))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("EXTERNAL PROCESS"));
    for expected in [
        "Kind: External",
        "PID: 44",
        "Command: aider --api-key=[REDACTED] --workspace /tmp/work",
        "Origin: external",
        "Signal: kill",
        "Target host: local",
    ] {
        assert!(text.contains(expected), "missing {expected} in {text}");
    }
    assert!(!text.contains("supersecret"));
    assert!(!text.contains("envsecret"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn external_agent_signal_requires_a_second_explicit_confirmation() {
    let _guard = SIGNAL_TEST_LOCK.lock().unwrap();
    SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(45, "aider", 10.0)]),
    ));
    app.resources.agent_signal_fn = test_signal;
    app.resources.agent_process_owned_fn = test_process_owned;

    assert!(app.resources_key(KeyCode::Char('k')));
    assert!(app.resources_key(KeyCode::Char('y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(app.resources.agent_signal_armed.is_some());
    assert!(app.resources_key(KeyCode::Char('Y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(app.resources.agent_signal_armed.is_none());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unowned_process_never_reaches_the_injected_signal_function() {
    let _guard = SIGNAL_TEST_LOCK.lock().unwrap();
    SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(46, "claude", 10.0)]),
    ));
    app.resources.agent_signal_fn = test_signal;
    app.resources.agent_process_owned_fn = never_owned;

    assert!(app.resources_key(KeyCode::Char('k')));
    assert!(app.resources_key(KeyCode::Char('y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(app.resources.agent_signal_armed.is_none());
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn process_signals_are_disabled_on_unsupported_platforms() {
    let _guard = SIGNAL_TEST_LOCK.lock().unwrap();
    SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(47, "claude", 10.0)]),
    ));
    app.resources.agent_signal_fn = test_signal;

    assert!(app.resources_key(KeyCode::Char('k')));
    assert!(app.resources.agent_signal_armed.is_none());
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(app.resources_key(KeyCode::Char('y')));
    assert_eq!(SIGNALS.load(std::sync::atomic::Ordering::SeqCst), 0);
}
