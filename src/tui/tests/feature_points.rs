//! Feature coverage for the dummy TokenMaxxing, daily bounty, and leaderboard UI.

use std::sync::Arc;

use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use medulla_tui::ui::app::{App, TABS};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

/// Draw the TokenMaxxing tab into a deterministic test terminal.
fn render_tokenmaxxing(width: u16, height: u16) -> String {
    let runtime = Arc::new(MockRuntime::empty());
    let mut app = App::new(runtime, LoadedConfig::defaults("medulla.tui.json".into()));
    app.tab_index = TABS
        .iter()
        .position(|name| *name == "TokenMaxxing")
        .expect("TokenMaxxing tab");

    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
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
fn tokenmaxxing_tab_shows_progress_bounties_and_daily_prizes() {
    let screen = render_tokenmaxxing(140, 40);

    for signature in [
        "TokenMaxxing",
        "2,480 pts",
        "520 pts to Level 8",
        "Daily bounties",
        "Ship something useful",
        "Daily leaderboard",
        "mira.dev",
        "1,000 pts + $25",
        "You",
    ] {
        assert!(
            screen.contains(signature),
            "missing {signature:?}: {screen}"
        );
    }
}

#[test]
fn tokenmaxxing_tab_keeps_the_core_program_readable_when_narrow() {
    let screen = render_tokenmaxxing(76, 32);

    assert!(screen.contains("Daily bounties"), "{screen}");
    assert!(screen.contains("Explorer bonus"), "{screen}");
    assert!(screen.contains("PLAYER"), "{screen}");
    assert!(screen.contains("You"), "{screen}");
    assert!(screen.contains("Settings"), "tab bar should fit: {screen}");
}
