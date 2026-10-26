//! Feature tests for the Settings tab: its grouped subpage nav, number-key and
//! arrow navigation, that every subpage renders, the Appearance theme editor
//! (live-applies + persists), and the unified themed selection highlight.
//!
//! Settings also hosts the Feedback board as a subpage,
//! so the tab bar's shrinking is asserted here too.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;

use medulla::backend::MockBackend;
use medulla::config::{LinkConfig, LoadedConfig};
use medulla_tui::ui::app::{App, Cmd, TABS};

fn loaded() -> LoadedConfig {
    let mut l = LoadedConfig::defaults("medulla.tui.json".into());
    l.config.link = Some(LinkConfig::default());
    l
}

fn settings_app() -> App {
    let rt = Arc::new(MockBackend::new());
    let mut app = App::new(rt, loaded());
    app.tab_index = TABS.iter().position(|t| *t == "Settings").unwrap();
    app
}

fn key(app: &mut App, code: KeyCode) -> Option<Cmd> {
    app.on_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    terminal.backend().buffer().clone()
}

fn text_of(buf: &Buffer) -> String {
    buf.content().iter().map(|c| c.symbol()).collect()
}

fn any_cell_with_bg(buf: &Buffer, bg: Color) -> bool {
    buf.content().iter().any(|c| c.bg == bg)
}

#[test]
fn settings_tab_renders_nav_and_default_usage_subpage() {
    let mut app = settings_app();
    let out = text_of(&draw(&mut app, 140, 40));
    // Left nav lists every subpage.
    for name in [
        "Usage",
        "Appearance",
        "Subscriptions",
        "Status line",
        "Config",
        "Feedback",
        "Account",
        "Help",
    ] {
        assert!(out.contains(name), "nav missing {name}: {out}");
    }
    // Default subpage is Usage.
    assert_eq!(app.settings_subpage(), "Usage");
    assert!(out.contains("Account"), "usage content: {out}");
}

#[test]
fn number_keys_jump_subpages() {
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('2'));
    assert_eq!(app.settings_subpage(), "Appearance");
    let _ = key(&mut app, KeyCode::Char('3'));
    assert_eq!(app.settings_subpage(), "Subscriptions");
    let _ = key(&mut app, KeyCode::Char('4'));
    assert_eq!(app.settings_subpage(), "Status line");
    let _ = key(&mut app, KeyCode::Char('5'));
    assert_eq!(app.settings_subpage(), "Config");
    let _ = key(&mut app, KeyCode::Char('6'));
    assert_eq!(app.settings_subpage(), "Resources");
    let _ = key(&mut app, KeyCode::Char('8'));
    assert_eq!(app.settings_subpage(), "Account");
    // A digit also enters the pane, so getting to Help means stepping back out
    // to the nav first — the arrows browse content once a page has focus.
    let _ = key(&mut app, KeyCode::Esc);
    let _ = key(&mut app, KeyCode::Down);
    assert_eq!(app.settings_subpage(), "Help");
    // Render the full help page: this test verifies numeric subpage navigation,
    // while short-viewport scrolling has its own focused coverage.
    let out = text_of(&draw(&mut app, 140, 64));
    assert!(out.contains("Keyboard & REPL help"), "help subpage: {out}");
    // Jumping to Usage requests an account-usage fetch.
    let cmd = key(&mut app, KeyCode::Char('1'));
    assert!(
        matches!(cmd, Some(Cmd::LoadUsage)),
        "usage jump loads usage"
    );
}

#[test]
fn arrow_keys_move_subpage_selector() {
    let mut app = settings_app();
    assert_eq!(app.settings_subpage(), "Usage");
    let _ = key(&mut app, KeyCode::Down);
    assert_eq!(app.settings_subpage(), "Appearance");
    let _ = key(&mut app, KeyCode::Down);
    assert_eq!(app.settings_subpage(), "Subscriptions");
    let _ = key(&mut app, KeyCode::Up);
    assert_eq!(app.settings_subpage(), "Appearance");
}

#[test]
fn status_line_selection_scrolls_into_view_on_a_short_terminal() {
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('4'));
    // Walk to the final path-style qualifier. Thread name and worktree each add
    // two rows ahead of the path group, so this must cover the complete
    // status-line catalog.
    for _ in 0..16 {
        let _ = key(&mut app, KeyCode::Down);
    }

    let out = text_of(&draw(&mut app, 80, 24));

    assert!(
        out.contains("shortened"),
        "the selected path-style value must remain visible: {out}"
    );
}

#[test]
fn a_status_line_field_is_headed_described_and_its_choices_spelled_out() {
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('4'));
    // Down four times: the harness group's "spelled" row.
    for _ in 0..4 {
        let _ = key(&mut app, KeyCode::Down);
    }

    let out = text_of(&draw(&mut app, 100, 40));

    assert!(
        out.contains("Harness name") && out.contains("which CLI is driving the session"),
        "the field is headed and described where its rows are: {out}"
    );
    for choice in ["long", "short", "icon"] {
        assert!(
            out.contains(choice),
            "the footer lists every value ←/→ can reach, not only the current one; \
             missing {choice}: {out}"
        );
    }
    assert!(
        out.contains("statusLine.harnessStyle"),
        "the footer names the key the answer is written to: {out}"
    );
    assert!(
        out.contains("Claude Code, claude, or just the provider icon."),
        "the footer explains what the selected row does: {out}"
    );
}

#[test]
fn appearance_cycling_changes_live_theme() {
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('2')); // Appearance
    assert_eq!(app.theme_primary(), Color::Red);
    // The primary role is selected first; Right enters the next editor color.
    let _ = key(&mut app, KeyCode::Right);
    assert_eq!(app.theme_primary(), Color::Cyan);
    // A selected row is now highlighted with the new primary background.
    let buf = draw(&mut app, 140, 40);
    assert!(
        any_cell_with_bg(&buf, Color::Cyan),
        "selection uses the live primary as background"
    );
    // Left steps back.
    let _ = key(&mut app, KeyCode::Left);
    assert_eq!(app.theme_primary(), Color::Red);
}

#[test]
fn appearance_jk_selects_role_before_cycling() {
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('2')); // Appearance
    let primary_before = app.theme_primary();
    // Move off the primary role, then cycle: primary must be untouched.
    let _ = key(&mut app, KeyCode::Char('j')); // select accent
    let _ = key(&mut app, KeyCode::Enter); // cycle accent
    assert_eq!(app.theme_primary(), primary_before, "primary unchanged");
}

#[test]
fn appearance_persists_theme_to_injected_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2')); // Appearance
    let _ = key(&mut app, KeyCode::Right); // cycle primary
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[theme]"), "theme section written: {text}");
    assert!(text.contains("primary = \"cyan\""), "primary saved: {text}");
    assert!(
        app.status().contains("saved"),
        "status note: {}",
        app.status()
    );
}

#[test]
fn appearance_blink_status_reports_the_boolean_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2'));
    for _ in 0..5 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Enter);

    assert!(app.status().contains("Attention blink → off (saved)"));
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(saved.contains("attentionBlink = false"), "{saved}");
}

/// The pulse rate is configured, reported, and persisted in seconds — the unit
/// the effect is actually judged in.
#[test]
fn appearance_cycles_the_attention_blink_rate_in_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2'));
    // Five colour rows and the blink toggle land on the rate.
    for _ in 0..6 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Right);

    assert!(
        app.status().contains("Attention blink rate → 1.5s (saved)"),
        "status note: {}",
        app.status()
    );
    let out = text_of(&draw(&mut app, 180, 45));
    assert!(out.contains("Blink rate           1.5s"), "{out}");
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(saved.contains("attentionBlinkSeconds = 1.5"), "{saved}");
}

/// Custom rates enter the offered cycle from the direction the operator chose.
#[test]
fn appearance_cycles_custom_attention_blink_rates_from_each_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path);
    app.set_attention_blink_ms(1_100);
    let _ = key(&mut app, KeyCode::Char('2'));
    for _ in 0..6 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }

    let _ = key(&mut app, KeyCode::Right);
    assert!(app.status().contains("Attention blink rate → 0.3s (saved)"));

    app.set_attention_blink_ms(1_100);
    let _ = key(&mut app, KeyCode::Left);
    assert!(app.status().contains("Attention blink rate → 3.0s (saved)"));

    let _ = key(&mut app, KeyCode::Left);
    assert!(app.status().contains("Attention blink rate → 2.0s (saved)"));
}

#[test]
fn appearance_cycles_and_persists_process_indicators() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2'));
    // Five color rows and the two attention controls precede resources.
    for _ in 0..7 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Right);

    let out = text_of(&draw(&mut app, 180, 45));
    assert!(out.contains("Process CPU          percent"), "{out}");
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(saved.contains("[appearance]"), "{saved}");
    assert!(saved.contains("cpu = \"percent\""), "{saved}");
    assert!(saved.contains("diskIo = \"off\""), "{saved}");
}

#[test]
fn appearance_cycles_and_persists_the_sidebar_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2'));
    // Five color rows, the two attention controls, and the seven option rows
    // before it.
    for _ in 0..14 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Right);

    let out = text_of(&draw(&mut app, 180, 60));
    assert!(out.contains("Group by             path"), "{out}");
    assert!(
        app.status().contains("Sidebar grouping \u{2192} path"),
        "status names the setting and its new value: {}",
        app.status()
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("sidebarGrouping = \"path\""), "{saved}");

    // The sort row is the next one down, and cycles independently.
    let _ = key(&mut app, KeyCode::Char('j'));
    let _ = key(&mut app, KeyCode::Right);

    let out = text_of(&draw(&mut app, 180, 60));
    assert!(out.contains("Sort by              recent"), "{out}");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("sidebarSort = \"recent\""), "{saved}");
    assert!(
        saved.contains("sidebarGrouping = \"path\""),
        "the grouping survives the next write: {saved}"
    );
}

#[test]
fn appearance_sidebar_grouping_wraps_backwards() {
    // Left from the default is the last value, not a stuck row: the cycle is
    // how every other control on this page behaves.
    let mut app = settings_app();
    let _ = key(&mut app, KeyCode::Char('2'));
    for _ in 0..14 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Left);

    let out = text_of(&draw(&mut app, 180, 60));
    assert!(out.contains("Group by             none"), "{out}");
}

#[test]
fn appearance_persists_process_indicators_to_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("medulla.tui.json");
    std::fs::write(&path, r#"{"unrelated":{"kept":true}}"#).unwrap();
    let mut app = settings_app();
    app.set_config_path(path.clone());
    let _ = key(&mut app, KeyCode::Char('2'));
    // Five color rows and the two attention controls precede resources.
    for _ in 0..7 {
        let _ = key(&mut app, KeyCode::Char('j'));
    }
    let _ = key(&mut app, KeyCode::Right);

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["appearance"]["cpu"], "percent");
    assert_eq!(saved["appearance"]["diskIo"], "off");
    assert_eq!(saved["unrelated"]["kept"], true);
}

/// A fixed reading, so the sidebar says the same thing on every machine.
mod appearance;
