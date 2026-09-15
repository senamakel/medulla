//! Render tests for the Hosts tab and the `H` switcher: the empty state, a
//! populated list with its detail, the pairing box, and the header label.

use std::sync::Arc;

use medulla::config::{LoadedConfig, RemoteHostSection};
use medulla::runtime::mock::MockRuntime;
use medulla::runtime::Runtime;

use crate::ui::app::hosts::HostSelection;
use crate::ui::app::types::tab_pos;
use crate::ui::app::App;

fn app_with(hosts: Vec<RemoteHostSection>) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let rt: Arc<dyn Runtime> = Arc::new(MockRuntime::demo());
    let mut loaded = LoadedConfig::defaults("medulla.tui.json".into());
    loaded.config.remote_hosts = hosts;
    let mut app = App::new(rt, loaded);
    app.set_config_path(dir.path().join("config.toml"));
    app.set_medulla_home(dir.path().join("home"));
    app.tab_index = tab_pos("Hosts");
    (dir, app)
}

fn render(app: &mut App) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn an_empty_page_says_how_to_add_a_host() {
    let (_dir, mut app) = app_with(Vec::new());
    let out = render(&mut app);
    assert!(out.contains("No hosts yet"), "{out}");
    assert!(
        out.contains("a add"),
        "the shortcut line names the add key: {out}"
    );
}

#[test]
fn a_populated_page_shows_the_list_the_detail_and_the_pairing_box() {
    let (_dir, mut app) = app_with(vec![RemoteHostSection {
        name: "GPU Box".to_string(),
        host: "gpu.example.com".to_string(),
        user: "ops".to_string(),
        udp_port: 60001,
        enabled: true,
        ..RemoteHostSection::default()
    }]);
    let _ = app.capture_clipboard();
    let out = render(&mut app);
    assert!(out.contains("GPU Box"), "{out}");
    assert!(out.contains("idle"), "{out}");
    assert!(out.contains("gpu.example.com"), "{out}");
    assert!(out.contains("60001"), "{out}");
    assert!(!out.contains("Run this once"), "no key has been issued yet");

    app.issue_host_key("gpu-box");
    let out = render(&mut app);
    assert!(out.contains("Run this once on the host"), "{out}");
    assert!(out.contains("medulla daemon HK1-"), "{out}");
    assert!(out.contains("s runs it over ssh"), "{out}");
}

#[test]
fn the_header_names_a_narrowed_selection_and_the_switcher_draws_its_rows() {
    let (_dir, mut app) = app_with(vec![RemoteHostSection {
        name: "Tower".to_string(),
        host: "tower.local".to_string(),
        udp_port: 60002,
        enabled: true,
        ..RemoteHostSection::default()
    }]);
    let out = render(&mut app);
    assert!(
        !out.contains("⌂"),
        "the combined default is not announced: {out}"
    );
    app.set_current_host(HostSelection::Host("tower".to_string()));
    let out = render(&mut app);
    assert!(out.contains("⌂ Tower"), "{out}");

    app.open_host_switcher();
    let out = render(&mut app);
    assert!(out.contains("Switch host"), "{out}");
    assert!(out.contains("All hosts"), "{out}");
    assert!(out.contains("Tower"), "{out}");
}
