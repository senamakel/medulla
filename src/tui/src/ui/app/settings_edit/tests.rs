//! Unit tests for the Config subpage's editable settings: what the rows read,
//! how they change, and what reaches disk.

use std::sync::Arc;

use medulla::backend::Backend;
use medulla::backend::MockBackend;
use medulla::config::{LoadedConfig, TuiConfig};

use super::super::types::App;
use super::{SettingKind, SettingValue};

/// An app over a bare mock runtime, with settings persisted into `dir`.
fn app_in(dir: &std::path::Path) -> App {
    let rt: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let mut app = App::new(rt, LoadedConfig::defaults("medulla.tui.json".into()));
    app.set_config_path(dir.join("config.toml"));
    app
}

/// The row index of `label`, so tests do not hard-code positions.
fn row_at(app: &App, label: &str) -> usize {
    app.config_rows()
        .iter()
        .position(|r| r.label == label)
        .unwrap_or_else(|| panic!("no {label} row"))
}

/// The config as it was written to disk.
fn written(dir: &std::path::Path) -> TuiConfig {
    let text = std::fs::read_to_string(dir.join("config.toml")).expect("config written");
    toml::from_str(&text).expect("valid toml")
}

#[test]
fn toggling_a_flag_applies_live_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    app.config_index = row_at(&app, "Update check");

    let rows = app.config_rows();
    let row = rows[app.config_index];
    assert_eq!(
        app.read_setting(&row),
        SettingValue::Flag(true),
        "on by default"
    );

    let status = app.adjust_setting(0);

    assert_eq!(
        app.read_setting(&row),
        SettingValue::Flag(false),
        "applied live"
    );
    assert!(
        status.contains("saved to"),
        "status names the target: {status}"
    );
    assert!(!written(dir.path()).update.check, "persisted");
}

#[test]
fn a_required_number_clamps_instead_of_going_auto() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    app.config_index = row_at(&app, "Worker concurrency");
    let row = app.config_rows()[app.config_index];

    for _ in 0..10 {
        app.adjust_setting(-1);
    }

    let SettingKind::Count { min, optional, .. } = row.kind else {
        panic!("Worker concurrency should be a count");
    };
    assert!(!optional, "the field has no unset state");
    assert_eq!(app.read_setting(&row), SettingValue::Number(min));
}

#[test]
fn numbers_clamp_at_their_maximum() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    app.config_index = row_at(&app, "Max depth");
    let row = app.config_rows()[app.config_index];
    let SettingKind::Count { max, .. } = row.kind else {
        panic!("Max depth should be a count");
    };

    for _ in 0..(max + 5) {
        app.adjust_setting(1);
    }

    assert_eq!(app.read_setting(&row), SettingValue::Number(max));
}

#[test]
fn enter_on_a_number_row_explains_itself_instead_of_changing_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    app.config_index = row_at(&app, "Max steps");
    let row = app.config_rows()[app.config_index];

    let status = app.adjust_setting(0);

    assert_eq!(app.read_setting(&row), SettingValue::Auto, "unchanged");
    assert!(
        status.contains("← or →"),
        "tells the user what to press: {status}"
    );
}

#[test]
fn without_a_config_path_changes_apply_live_but_say_they_are_not_saved() {
    let rt: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let mut app = App::new(rt, LoadedConfig::defaults("medulla.tui.json".into()));
    app.config_index = row_at(&app, "Update check");
    let row = app.config_rows()[app.config_index];

    let status = app.adjust_setting(0);

    assert_eq!(
        app.read_setting(&row),
        SettingValue::Flag(false),
        "applied live"
    );
    assert!(
        status.contains("no config path set"),
        "does not claim to have saved: {status}"
    );
}

#[test]
fn saving_warns_when_a_higher_precedence_file_still_wins() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    // The global file is written, but a project-local file is layered on top.
    app.loaded.sources = vec![
        dir.path()
            .join("config.toml")
            .to_string_lossy()
            .into_owned(),
        "./.medulla/config.toml".into(),
    ];
    app.config_index = row_at(&app, "Update check");

    let status = app.adjust_setting(0);

    assert!(
        status.contains("still overrides it"),
        "a shadowed write must say so: {status}"
    );
    assert!(
        status.contains("./.medulla/config.toml"),
        "names the winner: {status}"
    );
}

#[test]
fn the_cursor_stays_in_range_however_far_it_is_driven() {
    // Moving past the end must clamp rather than index off the list — the
    // failure mode is a panic on the next render, not a stale highlight.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    for _ in 0..50 {
        app.move_config_index(false);
    }
    assert_eq!(app.config_row_index(), app.config_rows().len() - 1);
}

#[cfg(feature = "workflows")]
#[test]
fn the_listed_run_cap_starts_at_its_default_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = app_in(dir.path());
    app.config_index = row_at(&app, "Runs listed");
    let row = app.config_rows()[app.config_index];
    let SettingKind::Count { step, .. } = row.kind else {
        panic!("Runs listed should be a count");
    };
    let default = medulla::config::WorkflowsConfig::DEFAULT_MAX_LISTED_RUNS as u32;
    assert_eq!(
        app.read_setting(&row),
        SettingValue::Number(default),
        "the row opens on the cap the page is already using"
    );

    app.adjust_setting(1);

    assert_eq!(app.read_setting(&row), SettingValue::Number(default + step));
    assert_eq!(
        written(dir.path()).workflows.max_listed_runs,
        (default + step) as usize,
        "persisted under [workflows]"
    );
}
