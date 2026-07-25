//! Deterministic tests for the event loop's asynchronous command dispatcher.

use std::sync::Arc;
use std::time::Duration;

use medulla::client::{FeedbackQuery, FeedbackType};
use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use medulla::runtime::{ContextItem, Runtime, RuntimeSnapshot, WorkerOp};
use medulla_tui::ui::app::{App, Cmd};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::cmd_dispatch::{read_memory, run_cmd};
use super::types::{AppMsg, EventLoopDeps, SessionExit, SessionWiring};
use super::update_checker::spawn_update_checker;
use super::{fold_app_msg, run_with, should_refresh_context};

/// Receive the next dispatcher result without allowing a broken task to hang
/// the entire test suite.
async fn next(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppMsg>) -> AppMsg {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("dispatcher timed out")
        .expect("dispatcher dropped its response channel")
}

struct FailingRuntime;

impl FailingRuntime {
    fn failure<T: Send + 'static>() -> futures::future::BoxFuture<'static, anyhow::Result<T>> {
        Box::pin(async { Err(anyhow::anyhow!("injected runtime failure")) })
    }
}

impl Runtime for FailingRuntime {
    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot::default()
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<()> {
        let (sender, receiver) = tokio::sync::broadcast::channel(1);
        sender.send(()).unwrap();
        sender.send(()).unwrap();
        receiver
    }

    fn submit(&self, _: String) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Self::failure()
    }

    fn abort(&self) {}

    fn new_session(&self) {}

    fn fork(&self, _: Option<String>) -> String {
        "failed".into()
    }

    fn set_active_thread(&self, _: String) {}

    fn list_main_chats(
        &self,
    ) -> futures::future::BoxFuture<
        'static,
        anyhow::Result<Vec<medulla::ui::chat_store::MainChatSummary>>,
    > {
        Self::failure()
    }

    fn resume_chat(&self, _: String) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Self::failure()
    }

    fn set_async_mode(&self, _: bool) -> bool {
        false
    }

    fn inspect_context(
        &self,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Vec<ContextItem>>> {
        Self::failure()
    }

    fn shutdown(&self) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Self::failure()
    }

    fn worker_op(&self, _: WorkerOp) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Self::failure()
    }

    fn team_usage(
        &self,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Option<serde_json::Value>>> {
        Self::failure()
    }

    fn list_feedback(
        &self,
        _: FeedbackQuery,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Option<medulla::client::FeedbackPage>>>
    {
        Self::failure()
    }
}

#[test]
fn context_refresh_tracks_the_nested_settings_page() {
    let runtime = Arc::new(MockRuntime::demo());
    let mut app = App::new(runtime, LoadedConfig::defaults("medulla.tui.json".into()));

    let _ = app.focus_settings_subpage("Usage");
    assert!(!should_refresh_context(&mut app));
    let _ = app.focus_settings_subpage("Context");
    assert!(should_refresh_context(&mut app));
    assert!(!should_refresh_context(&mut app));
}

#[test]
fn disabled_update_check_spawns_no_background_work() {
    let dir = tempfile::tempdir().unwrap();
    let env = std::collections::HashMap::new();
    let mut loaded = medulla::config::load_config(None, &env, dir.path()).unwrap();
    loaded.config.update.check = false;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    spawn_update_checker(&loaded, &tx);

    assert!(rx.try_recv().is_err());
}

fn session_wiring(directory: &tempfile::TempDir) -> SessionWiring {
    SessionWiring {
        loaded: LoadedConfig::defaults(directory.path().join("config.toml").display().to_string()),
        startup_status: Some("finite session".into()),
        tinyplace_obs: None,
        config_path: directory.path().join("config.toml"),
        medulla_home: directory.path().to_path_buf(),
        memory_service: None,
        sharing: None,
        onboarding_path: directory.path().join("config.toml"),
    }
}

fn deps(events: Vec<std::io::Result<crossterm::event::Event>>) -> EventLoopDeps {
    EventLoopDeps {
        events: Box::pin(futures::stream::iter(events)),
        ticks: Box::pin(futures::stream::empty()),
        check_updates: false,
        clear_credentials: Arc::new(|_| Ok(())),
        observe_draw: Arc::new(|_| {}),
    }
}

fn receiver_stream<T: Send + 'static>(
    receiver: tokio::sync::mpsc::UnboundedReceiver<T>,
) -> std::pin::Pin<Box<dyn futures::Stream<Item = T> + Send>> {
    Box::pin(futures::stream::unfold(receiver, |mut receiver| async {
        receiver.recv().await.map(|item| (item, receiver))
    }))
}

#[tokio::test]
async fn input_matrix_draws_first_frame_dispatches_key_and_ignores_non_key_input() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    let directory = tempfile::tempdir().unwrap();
    let concrete = Arc::new(MockRuntime::demo());
    let runtime: Arc<dyn Runtime> = concrete.clone();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let observations = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = observations.clone();
    let events = [
        Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
        Event::Paste("ignored".into()),
        Event::Resize(120, 40),
        Event::FocusLost,
        Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    ];
    let next_event = Arc::new(std::sync::Mutex::new(events.into_iter()));
    let event_source = next_event.clone();
    let mut loop_deps = deps(Vec::new());
    loop_deps.events = receiver_stream(event_rx);
    loop_deps.observe_draw = Arc::new(move |app| {
        observed.lock().unwrap().push((
            app.tab().to_string(),
            app.draft_text().to_string(),
            app.chat_scroll(),
            app.status().to_string(),
        ));
        if let Some(event) = event_source.lock().unwrap().next() {
            event_tx.send(Ok(event)).unwrap();
        }
    });

    assert_eq!(
        run_with(
            &mut terminal,
            runtime,
            session_wiring(&directory),
            loop_deps
        )
        .await
        .unwrap(),
        SessionExit::Quit
    );
    let seen = observations.lock().unwrap();
    assert_eq!(seen[0].0, "Overview");
    assert_eq!(seen[1].0, "Chat");
    assert_eq!(seen[2].2, 0);
    assert_eq!(
        &seen[2..=5],
        &[
            seen[1].clone(),
            seen[1].clone(),
            seen[1].clone(),
            seen[1].clone()
        ]
    );
    assert_eq!(seen[6].1, "h");
    assert_eq!(seen[7].3, "Cycle running…");
    assert!(terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|cell| !cell.symbol().trim().is_empty()));
    assert!(!concrete.snapshot().messages.is_empty());
}

#[tokio::test]
async fn ticks_advance_only_a_running_snapshot() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    for (running, expected) in [(false, 0), (true, 1)] {
        let directory = tempfile::tempdir().unwrap();
        let concrete = Arc::new(MockRuntime::empty());
        concrete.set_running(running);
        let runtime: Arc<dyn Runtime> = concrete;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (tick_tx, tick_rx) = tokio::sync::mpsc::unbounded_channel();
        let frames = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = frames.clone();
        let draws = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let draw_count = draws.clone();
        let mut loop_deps = deps(Vec::new());
        loop_deps.events = receiver_stream(event_rx);
        loop_deps.ticks = receiver_stream(tick_rx);
        loop_deps.observe_draw = Arc::new(move |app| {
            observed.lock().unwrap().push(app.frame);
            if draw_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                tick_tx.send(()).unwrap();
            } else {
                event_tx
                    .send(Ok(Event::Key(KeyEvent::new(
                        KeyCode::Char('c'),
                        KeyModifiers::CONTROL,
                    ))))
                    .unwrap();
            }
        });
        run_with(
            &mut terminal,
            runtime,
            session_wiring(&directory),
            loop_deps,
        )
        .await
        .unwrap();
        assert_eq!(frames.lock().unwrap()[1], expected);
    }
}

#[tokio::test]
async fn relogin_retires_credentials_through_the_session_boundary() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    let directory = tempfile::tempdir().unwrap();
    let runtime: Arc<dyn Runtime> = Arc::new(MockRuntime::empty());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let keys = std::iter::repeat_n(KeyCode::Tab, 6)
        .chain(std::iter::repeat_n(KeyCode::Down, 6))
        .chain(std::iter::repeat_n(KeyCode::Enter, 3));
    let events = Arc::new(std::sync::Mutex::new(keys));
    let source = events.clone();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let cleared = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let clear_count = cleared.clone();
    let mut loop_deps = deps(Vec::new());
    loop_deps.events = receiver_stream(event_rx);
    loop_deps.observe_draw = Arc::new(move |_| {
        if let Some(code) = source.lock().unwrap().next() {
            event_tx
                .send(Ok(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
                .unwrap();
        }
    });
    loop_deps.clear_credentials = Arc::new(move |_| {
        clear_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });

    let exit = run_with(
        &mut terminal,
        runtime,
        session_wiring(&directory),
        loop_deps,
    )
    .await
    .unwrap();
    assert_eq!(exit, SessionExit::Relogin);
    assert_eq!(cleared.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn sharing_reports_progress_persists_settlement_and_stops_polling_receiver() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use medulla_tui::ui::welcome::WelcomeEvent;

    let directory = tempfile::tempdir().unwrap();
    let runtime: Arc<dyn Runtime> = Arc::new(MockRuntime::empty());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let (share_tx, share_rx) = tokio::sync::mpsc::unbounded_channel();
    share_tx
        .send(WelcomeEvent::UploadProgress {
            uploaded: 1,
            total: 2,
            redactions: 3,
        })
        .unwrap();
    share_tx
        .send(WelcomeEvent::Claimed {
            awarded_usd: 2.0,
            tier: None,
            breakdown: Vec::new(),
            max_reward_usd: 5.0,
            already_claimed: false,
        })
        .unwrap();
    let mut wiring = session_wiring(&directory);
    wiring.sharing = Some(share_rx);
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let statuses = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = statuses.clone();
    let mut loop_deps = deps(Vec::new());
    loop_deps.events = receiver_stream(event_rx);
    loop_deps.observe_draw = Arc::new(move |app| {
        observed.lock().unwrap().push(app.status().to_string());
        if app.status().contains("free credits") {
            event_tx
                .send(Ok(Event::Key(KeyEvent::new(
                    KeyCode::Char('c'),
                    KeyModifiers::CONTROL,
                ))))
                .unwrap();
        }
    });

    run_with(&mut terminal, runtime, wiring, loop_deps)
        .await
        .unwrap();
    let seen = statuses.lock().unwrap();
    assert!(seen.iter().any(|status| status.contains("1/2 transcripts")));
    assert!(seen.iter().any(|status| status.contains("free credits")));
    assert!(
        std::fs::read_to_string(directory.path().join("config.toml"))
            .unwrap()
            .contains("welcome_completed = true")
    );
    drop(share_tx);
}

#[tokio::test]
async fn lagged_then_closed_runtime_subscription_does_not_starve_input() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    let directory = tempfile::tempdir().unwrap();
    let runtime: Arc<dyn Runtime> = Arc::new(FailingRuntime);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let exit = run_with(
        &mut terminal,
        runtime,
        session_wiring(&directory),
        deps(vec![Ok(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))]),
    )
    .await
    .unwrap();
    assert_eq!(exit, SessionExit::Quit);
}

#[tokio::test]
async fn finite_loop_treats_closed_input_as_quit_and_propagates_read_errors() {
    let directory = tempfile::tempdir().unwrap();
    let runtime: Arc<dyn Runtime> = Arc::new(MockRuntime::empty());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    let exit = run_with(
        &mut terminal,
        runtime.clone(),
        session_wiring(&directory),
        deps(Vec::new()),
    )
    .await
    .unwrap();
    assert_eq!(exit, SessionExit::Quit);

    let error = run_with(
        &mut terminal,
        runtime,
        session_wiring(&directory),
        deps(vec![Err(std::io::Error::other("input failed"))]),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("input failed"));
}

#[test]
fn background_messages_fold_state_and_request_required_followups() {
    let runtime = Arc::new(MockRuntime::demo());
    let mut app = App::new(runtime, LoadedConfig::defaults("test.toml".into()));

    assert!(fold_app_msg(&mut app, AppMsg::Status("working".into())).is_none());
    assert_eq!(app.status(), "working");

    assert!(fold_app_msg(
        &mut app,
        AppMsg::UsageLoaded(Some(serde_json::json!({"used": 7}))),
    )
    .is_none());
    assert_eq!(app.account_usage, Some(serde_json::json!({"used": 7})));

    assert!(fold_app_msg(
        &mut app,
        AppMsg::MemoryResults {
            hits: Vec::new(),
            query: "needle".into(),
        },
    )
    .is_none());
    assert_eq!(app.status(), "Memory · 0 hit(s)");

    let followup = fold_app_msg(&mut app, AppMsg::FeedbackChanged("saved".into()));
    assert!(matches!(followup, Some(Cmd::LoadFeedback(_))));

    assert!(fold_app_msg(&mut app, AppMsg::UpdateAvailable("v9 available".into()),).is_none());
    assert_eq!(app.status(), "v9 available");
}

#[test]
fn every_background_message_variant_folds_without_skipping_state_transitions() {
    use medulla::client::{FeedbackComment, FeedbackItem, FeedbackPage};
    use medulla::ui::chat_store::MainChatSummary;

    let runtime = Arc::new(MockRuntime::demo());
    let mut app = App::new(runtime, LoadedConfig::defaults("test.toml".into()));
    let item: FeedbackItem = serde_json::from_value(serde_json::json!({
        "id": "fb-1", "type": "feature", "title": "Title", "body": "Body",
        "status": "open", "createdByName": "Ada", "upvoteCount": 1,
        "downvoteCount": 0, "score": 1, "commentCount": 1, "myVote": 0,
        "createdAt": "2026-01-01T00:00:00Z"
    }))
    .unwrap();
    let comments = vec![FeedbackComment {
        id: "c-1".into(),
        user_name: Some("Ada".into()),
        body: "Useful".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    }];

    let messages = [
        AppMsg::Contexts(vec![ContextItem {
            ref_: "ctx".into(),
            kind: "file".into(),
            bytes: 4,
            content: "data".into(),
        }]),
        AppMsg::OpenResume(vec![MainChatSummary {
            session_id: "session".into(),
            name: "Chat".into(),
            turns: 1,
            thread_count: 1,
            updated_at: "now".into(),
        }]),
        AppMsg::Resumed("resumed".into()),
        AppMsg::MemoryLoaded {
            status: None,
            directives: vec!["Be concise".into()],
        },
        AppMsg::MemoryIngestDone("ingested".into()),
        AppMsg::TasksLoaded(medulla::tasks::TaskDocument::default()),
        AppMsg::FeedbackLoaded(Some(FeedbackPage {
            items: vec![item.clone()],
            total: 1,
        })),
        AppMsg::FeedbackComments {
            id: item.id.clone(),
            comments,
        },
        AppMsg::FeedbackItemUpdated(item),
    ];

    for message in messages {
        let _ = fold_app_msg(&mut app, message);
    }
    assert_eq!(app.status(), "Feedback · vote recorded");
    assert_eq!(app.feedback_items().len(), 1);
}

