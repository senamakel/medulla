//! Unit tests for workspace/action-dir derivation.
//!
//! These cover the derivation only, not [`super::boot`] — booting the core
//! touches process globals (`OnceLock` context, singleton event bus) and cannot
//! be torn down between tests. Boot coverage belongs in an integration test
//! with one core per test binary.

use super::*;

/// `bind_workspace` and `bind_action_dir` mutate process env, which is global.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn clear() {
    std::env::remove_var(OPENHUMAN_WORKSPACE_ENV);
    std::env::remove_var(OPENHUMAN_ACTION_DIR_ENV);
}

#[test]
fn workspace_nests_under_the_medulla_home() {
    // Nested, not a sibling: deleting a scratch MEDULLA_HOME must take the
    // core's state with it, or the next run silently inherits stale state.
    let dir = workspace_dir(Path::new("/tmp/scratch-home"));
    assert!(dir.starts_with("/tmp/scratch-home"), "{}", dir.display());
    assert!(dir.ends_with("openhuman/workspace"), "{}", dir.display());
}

#[test]
fn bind_workspace_derives_from_medulla_home_when_unset() {
    let _g = guard();
    clear();
    let env = HashMap::new();
    let bound = bind_workspace(&env, Path::new("/tmp/scratch-home"));
    assert_eq!(bound, workspace_dir(Path::new("/tmp/scratch-home")));
    assert_eq!(
        std::env::var(OPENHUMAN_WORKSPACE_ENV).unwrap(),
        bound.to_string_lossy()
    );
    clear();
}

#[test]
fn bind_workspace_keeps_an_explicit_operator_override() {
    // Aiming the embedded core at an existing OpenHuman install is a real
    // thing to want; the derivation must not stomp it.
    let _g = guard();
    clear();
    let env = HashMap::from([(
        OPENHUMAN_WORKSPACE_ENV.to_string(),
        "/opt/openhuman/ws".to_string(),
    )]);
    let bound = bind_workspace(&env, Path::new("/tmp/scratch-home"));
    assert_eq!(bound, PathBuf::from("/opt/openhuman/ws"));
    clear();
}

#[test]
fn bind_workspace_treats_a_blank_override_as_unset() {
    // An exported-but-empty var is a common shell accident; honouring it would
    // point the core at "" and break the scratch-run isolation silently.
    let _g = guard();
    clear();
    let env = HashMap::from([(OPENHUMAN_WORKSPACE_ENV.to_string(), "   ".to_string())]);
    let bound = bind_workspace(&env, Path::new("/tmp/scratch-home"));
    assert_eq!(bound, workspace_dir(Path::new("/tmp/scratch-home")));
    clear();
}

#[test]
fn action_dir_binds_to_the_operator_root() {
    let _g = guard();
    clear();
    let env = HashMap::new();
    let bound = bind_action_dir(&env, Some(Path::new("/repos/work")));
    assert_eq!(bound, Some(PathBuf::from("/repos/work")));
    assert_eq!(
        std::env::var(OPENHUMAN_ACTION_DIR_ENV).unwrap(),
        "/repos/work"
    );
    clear();
}

#[test]
fn action_dir_left_alone_without_a_root() {
    // Better to leave OpenHuman's own default than to bind something arbitrary.
    let _g = guard();
    clear();
    let env = HashMap::new();
    assert_eq!(bind_action_dir(&env, None), None);
    assert!(std::env::var(OPENHUMAN_ACTION_DIR_ENV).is_err());
    clear();
}

#[test]
fn action_dir_keeps_an_explicit_override() {
    let _g = guard();
    clear();
    let env = HashMap::from([(
        OPENHUMAN_ACTION_DIR_ENV.to_string(),
        "/explicit".to_string(),
    )]);
    let bound = bind_action_dir(&env, Some(Path::new("/repos/work")));
    assert_eq!(bound, Some(PathBuf::from("/explicit")));
    clear();
}
