//! The Resources tab against the real, embedded btop.
//!
//! btop runs once per process and every `App` shares it, so the lifecycle is
//! one test walked end to end; key routing that never reaches btop is tested
//! on its own.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use medulla::backend::{Backend, MockBackend};
use medulla::config::LoadedConfig;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::ui::app::types::tab_pos;
use crate::ui::app::App;

fn app_on_resources() -> App {
    let backend: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let mut app = App::new(backend, LoadedConfig::defaults("medulla.tui.json".into()));
    app.tab_index = tab_pos("Resources");
    app
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.on_key(KeyEvent::new(code, modifiers));
}

/// Redraw until the screen satisfies `done`, as the event loop would on
/// btop's wakes; false if it never does.
fn draw_until(app: &mut App, terminal: &mut Terminal<TestBackend>, done: impl Fn(&App, &str) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        terminal.draw(|f| app.draw(f)).expect("draw");
        let screen = crate::ui::btop::pane().contents();
        if done(app, &screen) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn tab_switching_and_global_chords_stay_medullas() {
    let mut app = app_on_resources();
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_ne!(app.tab(), "Resources", "Tab must still switch tabs");

    let mut app = app_on_resources();
    press(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_ne!(app.tab(), "Resources", "Shift-Tab must still switch tabs");

    let mut app = app_on_resources();
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(app.should_quit, "⌃C must still quit");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_tab_runs_btop_focused_on_agents_and_q_returns_to_sessions() {
    let mut app = app_on_resources();
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");

    // btop draws its boxes, with the process box narrowed to coding agents.
    assert!(
        draw_until(&mut app, &mut terminal, |_, screen| screen.contains("cpu")
            && screen.contains("agents")),
        "btop never drew: {}",
        crate::ui::btop::pane().contents()
    );
    assert!(crate::ui::btop::pane().running());
    // What btop drew is what the frame shows, below the tab bar.
    let frame: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(frame.contains("agents"));

    // `A` is btop's agents toggle; the title follows it.
    press(&mut app, KeyCode::Char('A'), KeyModifiers::NONE);
    assert!(
        draw_until(&mut app, &mut terminal, |_, screen| screen.contains("proc")
            && !screen.contains("agents")),
        "A did not switch to all processes"
    );
    press(&mut app, KeyCode::Char('A'), KeyModifiers::NONE);
    assert!(draw_until(&mut app, &mut terminal, |_, screen| screen
        .contains("agents")));

    // btop's quit is "leave the tab", not "end Medulla".
    press(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(
        draw_until(&mut app, &mut terminal, |app, _| app.tab() == "Sessions"),
        "q did not return to Sessions"
    );
    assert!(!app.should_quit);
    // Off screen, btop is paused, and coming back resumes it.
    assert!(!crate::ui::btop::pane().visible);
    app.tab_index = tab_pos("Resources");
    assert!(draw_until(&mut app, &mut terminal, |_, _| crate::ui::btop::pane()
        .visible));
}
