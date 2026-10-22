use super::{AgentRegistration, AgentRegistry};

fn origin(task_id: Option<&str>, session_id: Option<&str>, provider: &str) -> AgentRegistration {
    AgentRegistration {
        pid: 17,
        started_at_secs: 100,
        task_id: task_id.map(str::to_owned),
        session_id: session_id.map(str::to_owned),
        provider: provider.to_owned(),
    }
}

#[test]
fn registration_is_visible_with_its_task_session_and_provider_origin() {
    let registry = AgentRegistry::default();
    let guard = registry.register_at(origin(Some("task-1"), Some("session-2"), "claude"));

    assert_eq!(
        registry.snapshot(),
        vec![origin(Some("task-1"), Some("session-2"), "claude")]
    );
    drop(guard);
    assert!(registry.snapshot().is_empty());
}

#[test]
fn stale_pid_registration_is_not_a_known_root_after_pid_reuse() {
    let registry = AgentRegistry::default();
    let _guard = registry.register_at(origin(Some("task-1"), None, "codex"));

    assert!(registry.current_for(17, 101).is_none());
    assert_eq!(
        registry.current_for(17, 100),
        Some(origin(Some("task-1"), None, "codex"))
    );
}

#[test]
fn dropping_an_old_guard_does_not_remove_a_newer_pid_identity() {
    let registry = AgentRegistry::default();
    let old = registry.register_at(origin(Some("old"), None, "claude"));
    let mut replacement = origin(Some("new"), None, "codex");
    replacement.started_at_secs = 101;
    let _new = registry.register_at(replacement.clone());

    drop(old);

    assert_eq!(registry.snapshot(), vec![replacement]);
}

#[test]
fn dropping_an_old_guard_does_not_remove_a_reregistered_process_identity() {
    let registry = AgentRegistry::default();
    let old = registry.register_at(origin(Some("old"), None, "claude"));
    let mut replacement = origin(Some("new"), Some("session"), "codex");
    replacement.started_at_secs = 100;
    let _new = registry.register_at(replacement.clone());

    drop(old);

    assert_eq!(registry.snapshot(), vec![replacement]);
}
