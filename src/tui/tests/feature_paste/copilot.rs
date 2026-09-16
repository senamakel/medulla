//! Paste into the Workflows copilot — the app's second multiline composer, and
//! the one a long instruction is most likely to be pasted into.

#![cfg(feature = "workflows")]

use crate::helpers::*;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

/// A Workflows-tab app reading a store scoped to `home`.
///
/// Scoped deliberately: the store a real session resolves also reads the
/// current directory's `.medulla/workflows`, which would make the test
/// depend on the developer's own catalogue.
fn workflows_app(home: &std::path::Path) -> App {
    let mut app = App::new(Arc::new(MockBackend::new()), loaded());
    app.set_medulla_home(home.to_path_buf());
    app.set_workflow_store(Arc::new(medulla::workflows::FileWorkflowStore::new(
        vec![home.join("workflows")],
        home.join("state").join("workflows").join("runs"),
    )));
    app.tab_index = tab_index("Workflows");
    app
}

fn rendered(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(150, 34)).expect("test terminal");
    terminal.draw(|f| app.draw(f)).expect("draws");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn a_paste_lands_in_the_copilot_composer_with_its_line_breaks() {
    let home = tempfile::tempdir().expect("temp home");
    let mut app = workflows_app(home.path());
    // `c` is the binding that reaches the copilot from the catalogue.
    key_press(&mut app, KeyCode::Char('c'));

    assert!(paste(&mut app, "run the sweep\r\nthen notify me").is_none());

    let screen = rendered(&mut app);
    assert!(
        screen.contains("run the sweep") && screen.contains("then notify me"),
        "the whole instruction is in the composer: {screen}"
    );
}

#[test]
fn a_paste_on_the_catalogue_is_ignored_rather_than_stored_out_of_sight() {
    // The sidebar is a list, not a field. Its only draft is the copilot's,
    // one pane over and not on screen for this focus.
    let home = tempfile::tempdir().expect("temp home");
    let mut app = workflows_app(home.path());

    assert!(paste(&mut app, "stray text").is_none());

    key_press(&mut app, KeyCode::Char('c'));
    let screen = rendered(&mut app);
    assert!(
        !screen.contains("stray text"),
        "nothing was banked into the copilot behind the list: {screen}"
    );
}
