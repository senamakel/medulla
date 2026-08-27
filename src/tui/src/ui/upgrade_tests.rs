//! Unit tests for the paywall screen's state machine.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use medulla::access::Denial;

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn screen() -> UpgradeScreen {
    UpgradeScreen::new("Logged in as a@b.c (u1)", Denial::TrialExpired)
}

#[test]
fn it_starts_with_no_outcome() {
    assert_eq!(screen().outcome(), None);
}

#[test]
fn subscribe_opens_the_pricing_page_without_leaving() {
    let mut s = screen();

    let cmd = s.handle_key(key(KeyCode::Enter));

    assert_eq!(cmd, Some(UpgradeCmd::OpenUrl(PRICING_URL.to_string())));
    // Opening the page is not a way out of the screen: the account still has no
    // plan until the checkout completes and the next launch re-checks.
    assert_eq!(s.outcome(), None);
}

#[test]
fn the_quit_row_leaves() {
    let mut s = screen();

    assert_eq!(s.handle_key(key(KeyCode::Down)), None);
    assert_eq!(s.handle_key(key(KeyCode::Enter)), None);

    assert_eq!(s.outcome(), Some(UpgradeOutcome::Quit));
}

#[test]
fn esc_leaves_because_there_is_nothing_to_back_out_to() {
    let mut s = screen();
    s.handle_key(key(KeyCode::Esc));
    assert_eq!(s.outcome(), Some(UpgradeOutcome::Quit));
}

#[test]
fn ctrl_c_leaves_from_any_row() {
    let mut s = screen();
    s.handle_key(key(KeyCode::Down));
    s.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(s.outcome(), Some(UpgradeOutcome::Quit));
}

#[test]
fn the_selection_wraps_in_both_directions() {
    let mut s = screen();

    // Up from the first row lands on Quit, which Enter then takes.
    s.handle_key(key(KeyCode::Up));
    s.handle_key(key(KeyCode::Enter));

    assert_eq!(s.outcome(), Some(UpgradeOutcome::Quit));
}

/// There is no key that admits the operator to the app. If one is ever added,
/// this fails — which is the point.
#[test]
fn no_keystroke_dismisses_the_screen_into_the_app() {
    let codes = [
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Char('y'),
        KeyCode::Char(' '),
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
    ];
    for code in codes {
        let mut s = screen();
        s.handle_key(key(code));
        assert!(
            matches!(s.outcome(), None | Some(UpgradeOutcome::Quit)),
            "{code:?} produced a non-quit outcome"
        );
    }
}

/// Render the panel and return its cells as one string.
fn render(s: &mut UpgradeScreen) -> String {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
    terminal.draw(|frame| s.draw(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn the_panel_says_what_happened_who_it_is_about_and_where_to_go() {
    let out = render(&mut screen());

    assert!(
        out.contains("Your free week is up."),
        "the reason is missing"
    );
    assert!(out.contains("a@b.c"), "the account is not named");
    assert!(out.contains(PRICING_URL), "the pricing URL is missing");
    assert!(out.contains("Subscribe"), "the subscribe row is missing");
    assert!(out.contains("subscribe"), "the panel title is missing");
}

/// The operator may be a paying customer whose plan we failed to look up.
/// Blaming their trial for our outage would be both wrong and insulting.
#[test]
fn an_unconfirmed_plan_does_not_claim_the_trial_expired() {
    let mut s = UpgradeScreen::new("backend https://api.tinyhumans.ai", Denial::Undetermined);

    let out = render(&mut s);

    assert!(
        out.contains("Your plan could not be confirmed."),
        "the unconfirmed headline is missing"
    );
    assert!(
        !out.contains("free week"),
        "an unconfirmed plan must not be reported as a spent trial"
    );
    assert!(
        out.contains("try again"),
        "the operator is not told what to do next"
    );
    // Still the way back to a plan, for somebody who genuinely has none.
    assert!(out.contains(PRICING_URL), "the pricing URL is missing");
}

#[test]
fn the_opened_flash_appears_after_subscribing() {
    let mut s = screen();
    assert!(!render(&mut s).contains("opened"));

    s.handle_key(key(KeyCode::Enter));

    assert!(render(&mut s).contains("opened"));
}
