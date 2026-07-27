//! Feature coverage for the dummy TokenMaxxing, daily bounty, and leaderboard UI.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use medulla_tui::ui::app::{App, TABS};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;

/// A key press with no modifiers.
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Build an app already positioned on TokenMaxxing.
fn tokenmaxxing_app() -> App {
    let runtime = Arc::new(MockRuntime::empty());
    let mut app = App::new(runtime, LoadedConfig::defaults("medulla.tui.json".into()));
    app.tab_index = TABS
        .iter()
        .position(|name| *name == "TokenMaxxing")
        .expect("TokenMaxxing tab");
    app
}

/// Draw an app into a deterministic test terminal.
fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

/// Draw one numbered TokenMaxxing sidebar page.
fn render_tokenmaxxing(page: char, width: u16, height: u16) -> Buffer {
    let mut app = tokenmaxxing_app();
    if page != '1' {
        assert!(app.on_event(key(KeyCode::Char(page))).is_none());
    }
    render(&mut app, width, height)
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
fn overview_page_shows_progress_and_season_leaderboard() {
    let screen = screen_text(&render_tokenmaxxing('1', 140, 40));

    for signature in [
        "TokenMaxxing",
        "2,480 pts",
        "9.8M tokens burned",
        "520 pts to Level 8",
        "Season leaderboard",
        "@mira-dev",
        "48.2M",
        "@you",
    ] {
        assert!(
            screen.contains(signature),
            "missing {signature:?}: {screen}"
        );
    }
}

#[test]
fn bounties_page_shows_challenges_progress_and_rules() {
    let screen = screen_text(&render_tokenmaxxing('2', 140, 40));

    for signature in [
        "120 pts claimed",
        "Daily bounties",
        "Ship something useful",
        "Explorer bonus",
        "How daily bounties work",
    ] {
        assert!(
            screen.contains(signature),
            "missing {signature:?}: {screen}"
        );
    }
}

#[test]
fn dedicated_leaderboard_page_reuses_the_daily_standings() {
    let screen = screen_text(&render_tokenmaxxing('3', 140, 40));

    assert!(screen.contains("Season leaderboard"), "{screen}");
    assert!(screen.contains("@mira-dev"), "{screen}");
    assert!(screen.contains("Rewards"), "{screen}");
    assert!(screen.contains("Daily token burner"), "{screen}");
    assert!(screen.contains("$250 + TokenMaxxer badge"), "{screen}");
    assert!(screen.contains("Previous daily winners"), "{screen}");
    assert!(screen.contains("Jul 26"), "{screen}");
    assert!(screen.contains("$25 + 1,000 pts"), "{screen}");
    assert!(!screen.contains("Daily bounties"), "{screen}");
}

#[test]
fn sidebar_navigation_switches_pages_and_focus() {
    let mut app = tokenmaxxing_app();
    assert!(app.on_event(key(KeyCode::Down)).is_none());
    let bounties = screen_text(&render(&mut app, 120, 32));
    assert!(bounties.contains("How daily bounties work"), "{bounties}");
    assert!(bounties.contains("1-3 jump"), "{bounties}");

    assert!(app.on_event(key(KeyCode::Enter)).is_none());
    assert!(app.status().contains("Bounties · Esc"));
    assert!(app.on_event(key(KeyCode::Esc)).is_none());
    assert_eq!(app.status(), "TokenMaxxing · menu");
}

#[test]
fn tokenmaxxing_tab_keeps_every_page_reachable_when_narrow() {
    let overview = screen_text(&render_tokenmaxxing('1', 76, 32));
    assert!(overview.contains("Overview"), "{overview}");
    assert!(overview.contains("Bounties"), "{overview}");
    assert!(overview.contains("Leaderboard"), "{overview}");
    assert!(overview.contains("@you"), "{overview}");
    assert!(
        overview.contains("Settings"),
        "tab bar should fit: {overview}"
    );

    let bounties = screen_text(&render_tokenmaxxing('2', 76, 32));
    assert!(bounties.contains("Explorer bonus"), "{bounties}");
    let leaderboard = screen_text(&render_tokenmaxxing('3', 76, 32));
    assert!(leaderboard.contains("GITHUB"), "{leaderboard}");
    assert!(leaderboard.contains("@you"), "{leaderboard}");
    assert!(
        leaderboard.contains("Previous daily winners"),
        "{leaderboard}"
    );
}

#[test]
fn secondary_title_copy_uses_an_explicit_color_without_dim_support() {
    let overview = render_tokenmaxxing('1', 140, 40);
    for text in ["Season 01", "GitHub users ranked"] {
        let index = cell_index_of(&overview, text).expect("secondary title text");
        assert_eq!(
            overview.content()[index].fg,
            Color::DarkGray,
            "{text:?} must not rely on the inconsistently supported ANSI DIM attribute"
        );
    }
    let bounties = render_tokenmaxxing('2', 140, 40);
    let text = "refreshes in";
    let index = cell_index_of(&bounties, text).expect("secondary title text");
    assert_eq!(
        bounties.content()[index].fg,
        Color::DarkGray,
        "{text:?} must not rely on the inconsistently supported ANSI DIM attribute"
    );
}
