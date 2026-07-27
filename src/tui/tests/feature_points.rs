//! Feature coverage for the dummy TokenMaxxing, daily bounty, and leaderboard UI.

use std::sync::Arc;

use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use medulla_tui::ui::app::{App, TABS};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;

/// Draw the TokenMaxxing tab into a deterministic test terminal.
fn render_tokenmaxxing(width: u16, height: u16) -> Buffer {
    let runtime = Arc::new(MockRuntime::empty());
    let mut app = App::new(runtime, LoadedConfig::defaults("medulla.tui.json".into()));
    app.tab_index = TABS
        .iter()
        .position(|name| *name == "TokenMaxxing")
        .expect("TokenMaxxing tab");

    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

/// Flatten a terminal buffer for copy-level assertions.
fn screen_text(buffer: &Buffer) -> String {
    buffer.content().iter().map(|cell| cell.symbol()).collect()
}

/// Find the starting cell of `needle`, accounting for multi-byte cell symbols.
fn cell_index_of(buffer: &Buffer, needle: &str) -> Option<usize> {
    let symbols: Vec<&str> = buffer.content().iter().map(|cell| cell.symbol()).collect();
    (0..symbols.len()).find(|start| {
        let mut seen = String::new();
        for symbol in &symbols[*start..] {
            seen.push_str(symbol);
            if seen.len() >= needle.len() {
                break;
            }
        }
        seen.starts_with(needle)
    })
}

#[test]
fn tokenmaxxing_tab_shows_progress_bounties_and_daily_prizes() {
    let screen = screen_text(&render_tokenmaxxing(140, 40));

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
    let screen = screen_text(&render_tokenmaxxing(76, 32));

    assert!(screen.contains("Daily bounties"), "{screen}");
    assert!(screen.contains("Explorer bonus"), "{screen}");
    assert!(screen.contains("PLAYER"), "{screen}");
    assert!(screen.contains("You"), "{screen}");
    assert!(screen.contains("Settings"), "tab bar should fit: {screen}");
}

#[test]
fn secondary_title_copy_uses_an_explicit_color_without_dim_support() {
    let buffer = render_tokenmaxxing(140, 40);

    for text in ["Season 01", "refreshes in", "closes at"] {
        let index = cell_index_of(&buffer, text).expect("secondary title text");
        assert_eq!(
            buffer.content()[index].fg,
            Color::DarkGray,
            "{text:?} must not rely on the inconsistently supported ANSI DIM attribute"
        );
    }
}
