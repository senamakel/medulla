//! Feature coverage for Tasks-tab navigation, prompts, CRUD commands, and sync.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use medulla::tasks::{SourceConfig, Task, TaskDocument, TaskStatus};
use medulla_tui::ui::app::{App, Cmd, TABS};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn app() -> App {
    App::new(
        Arc::new(MockRuntime::empty()),
        LoadedConfig::defaults("medulla.tui.json".into()),
    )
}

fn task(id: &str, title: &str) -> Task {
    Task {
        id: id.into(),
        title: title.into(),
        description: String::new(),
        status: TaskStatus::Open,
        source: None,
        recurrence: None,
        created_at: "1".into(),
        updated_at: "1".into(),
        last_synced_at: None,
        dispatch: serde_json::Value::Null,
    }
}

fn focus_tasks(app: &mut App) {
    app.tab_index = TABS.iter().position(|tab| *tab == "Tasks").unwrap();
    app.on_event(key(KeyCode::Enter));
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        assert!(app.on_event(key(KeyCode::Char(character))).is_none());
    }
}

fn render(app: &mut App) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn entering_tasks_requests_the_local_document() {
    let mut app = app();
    app.tab_index = TABS.iter().position(|tab| *tab == "Agents").unwrap();
    assert!(matches!(
        app.on_event(key(KeyCode::Tab)),
        Some(Cmd::LoadTasks)
    ));
    assert_eq!(app.tab(), "Tasks");
}

#[test]
fn create_and_edit_prompts_emit_task_saves() {
    let mut app = app();
    focus_tasks(&mut app);

    assert!(app.on_event(key(KeyCode::Char('a'))).is_none());
    type_text(&mut app, "Write tests");
    let Some(Cmd::SaveTask(created)) = app.on_event(key(KeyCode::Enter)) else {
        panic!("create prompt should save a task");
    };
    assert_eq!(created.title, "Write tests");

    app.set_tasks(TaskDocument {
        tasks: vec![task("task-1", "Old title")],
        sources: vec![],
    });
    assert!(app.on_event(key(KeyCode::Char('e'))).is_none());
    for _ in 0.."Old title".len() {
        app.on_event(key(KeyCode::Backspace));
    }
    type_text(&mut app, "New title");
    let Some(Cmd::SaveTask(edited)) = app.on_event(key(KeyCode::Enter)) else {
        panic!("edit prompt should save the selected task");
    };
    assert_eq!(edited.id, "task-1");
    assert_eq!(edited.title, "New title");

    app.set_tasks(TaskDocument {
        tasks: vec![task("removed", "Removed task")],
        sources: vec![],
    });
    assert!(app.on_event(key(KeyCode::Char('e'))).is_none());
    app.set_tasks(TaskDocument::default());
    assert!(app.on_event(key(KeyCode::Enter)).is_none());
    assert_eq!(app.status(), "Tasks · task no longer exists");
}

#[test]
fn selection_delete_and_sync_follow_the_visible_task_state() {
    let mut app = app();
    focus_tasks(&mut app);
    app.set_tasks(TaskDocument {
        tasks: vec![task("first", "First"), task("second", "Second")],
        sources: vec![],
    });

    assert!(app.on_event(key(KeyCode::Down)).is_none());
    assert!(matches!(
        app.on_event(key(KeyCode::Char('d'))),
        Some(Cmd::DeleteTask(id)) if id == "second"
    ));
    assert!(app.on_event(key(KeyCode::Esc)).is_none());
    assert!(app.on_event(key(KeyCode::Char('2'))).is_none());
    assert!(app.on_event(key(KeyCode::Char('s'))).is_none());
    assert_eq!(app.status(), "Sources · none configured");

    app.set_tasks(TaskDocument {
        tasks: vec![],
        sources: vec![SourceConfig {
            id: "github".into(),
            provider: "github".into(),
            enabled: true,
            repository: "tinyhumansai/medulla".into(),
            state: "open".into(),
            labels: vec![],
            filter: None,
            token: None,
        }],
    });
    assert!(matches!(
        app.on_event(key(KeyCode::Char('s'))),
        Some(Cmd::SyncTasks(id)) if id == "github"
    ));
}

#[test]
fn source_prompt_persists_a_github_configuration() {
    let mut app = app();
    focus_tasks(&mut app);

    assert!(app.on_event(key(KeyCode::Esc)).is_none());
    assert!(app.on_event(key(KeyCode::Char('2'))).is_none());
    assert_eq!(app.tasks_subpage(), "Sources");
    assert!(app.on_event(key(KeyCode::Char('a'))).is_none());
    type_text(&mut app, "tinyhumansai/medulla");
    let Some(Cmd::SaveTasks(document)) = app.on_event(key(KeyCode::Enter)) else {
        panic!("source prompt should save the task document");
    };
    assert_eq!(document.sources.len(), 1);
    assert_eq!(document.sources[0].repository, "tinyhumansai/medulla");
    assert_eq!(document.sources[0].provider, "github");
}

#[test]
fn tasks_use_the_shared_subpage_menu() {
    let mut app = app();
    app.tab_index = TABS.iter().position(|tab| *tab == "Tasks").unwrap();

    assert_eq!(app.tasks_subpage(), "All Tasks");
    assert!(!app.tasks_focused());
    assert!(app.on_event(key(KeyCode::Down)).is_none());
    assert_eq!(app.tasks_subpage(), "Sources");
    assert!(app.on_event(key(KeyCode::Enter)).is_none());
    assert!(app.tasks_focused());
    assert!(app.on_event(key(KeyCode::Esc)).is_none());
    assert!(!app.tasks_focused());
}

#[test]
fn selected_task_status_uses_one_continuous_highlight() {
    let mut app = app();
    focus_tasks(&mut app);
    app.set_tasks(TaskDocument {
        tasks: vec![task("styled", "Styled task")],
        sources: vec![],
    });

    let buffer = render(&mut app);
    let status_bracket = buffer
        .content()
        .iter()
        .find(|cell| cell.symbol() == "[")
        .expect("rendered task status");
    assert_eq!(
        status_bracket.bg,
        Color::Cyan,
        "the status suffix should share the selected row background"
    );
}
