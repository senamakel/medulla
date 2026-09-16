//! Feature tests for the feedback submission prompt's empty-input cancels.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use medulla::backend::MockBackend;
use medulla::config::LoadedConfig;
use medulla_tui::ui::app::App;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_str(app: &mut App, s: &str) {
    for ch in s.chars() {
        app.on_event(key(KeyCode::Char(ch)));
    }
}

fn app() -> App {
    App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("medulla.tui.json".into()),
    )
}

#[test]
fn an_empty_feedback_title_cancels_the_submission() {
    // The prompt is two steps; submitting nothing at either step must abandon
    // the whole thing rather than post a blank item.
    let mut app = app();
    let _ = app.focus_settings_subpage("Feedback");
    app.on_event(key(KeyCode::Char('n'))); // new feature request
    let cmd = app.on_event(key(KeyCode::Enter)); // …with no title
    assert!(cmd.is_none(), "nothing is submitted");
    assert!(
        app.status().contains("cancelled"),
        "says it was cancelled: {}",
        app.status()
    );
}

#[test]
fn an_empty_feedback_description_cancels_the_submission() {
    let mut app = app();
    let _ = app.focus_settings_subpage("Feedback");
    app.on_event(key(KeyCode::Char('b'))); // new bug report
    type_str(&mut app, "a real title");
    app.on_event(key(KeyCode::Enter)); // advances to the description step
    let cmd = app.on_event(key(KeyCode::Enter)); // …with no description
    assert!(cmd.is_none(), "nothing is submitted");
    assert!(
        app.status().contains("cancelled"),
        "says it was cancelled: {}",
        app.status()
    );
}
