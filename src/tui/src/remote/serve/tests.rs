//! Tests for the serving loop, driven over an in-process bridge.
//!
//! These start **real ptys running real shells** and speak the real wire
//! protocol to them; only the transport is swapped, for a `LocalBridgeNetwork`
//! that delivers in memory. So everything the relay actually does — resolving a
//! choice, launching, sampling the emulator, writing input back — is exercised
//! here, and what the Docker suite adds on top is the SSH bootstrap, the UDP
//! link, and two machines.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use medulla::bridge::{Bridge, LocalBridgeNetwork};
use medulla::protocol::{
    encode_remote_message, encode_screen_message, parse_remote_message, HarnessProvider,
    RemoteHarnessChoice, RemoteMessage, ScreenMessage,
};

use super::*;
use crate::worker::pty::PtyManager;

/// The client's address on the in-process bridge.
const CLIENT: &str = "client";
/// The daemon's.
const DAEMON: &str = "daemon";

/// A `LocalSessions` that can start a real shell in a real directory.
fn shell_sessions(workspace: &str) -> LocalSessions {
    let config = medulla::daemon::DaemonConfig {
        hooks: medulla::harness_hooks::HooksConfig::default(),
        providers: Vec::new(),
        default_provider: HarnessProvider::Shell,
        workspace: workspace.to_string(),
        accessible_dirs: Vec::new(),
        env: HashMap::new(),
        task_timeout_ms: 1_000,
        capability_timeout_ms: None,
        concurrency: 1,
        status_throttle_ms: 1_000,
        max_pending: 1,
        model: None,
        agent: None,
        extra_args: Vec::new(),
        skip_permissions: false,
        router: None,
        custom_harnesses: Vec::new(),
        budget: None,
        attribution: true,
    };
    let run_task: medulla::daemon::providers::RunTaskFn =
        Arc::new(|_| Box::pin(async { Err("unused".to_string()) }));
    let send: medulla::daemon::SendFn = Arc::new(|_, _| Box::pin(async {}));
    let mut env: HashMap<String, String> = HashMap::new();
    if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".to_string(), path);
    }
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    // Pin the shell so the test does not inherit whatever `$SHELL` the developer
    // happens to run; `sh` exists everywhere this suite does.
    env.insert("MEDULLA_SHELL_BIN".to_string(), "/bin/sh".to_string());
    // A bare prompt, so assertions match output rather than someone's `$PS1`.
    env.insert("PS1".to_string(), "$ ".to_string());
    LocalSessions {
        hooks: medulla::harness_hooks::HooksConfig::default(),
        log: None,
        sessions: PtyManager::new(),
        runtimes: Arc::new(std::sync::Mutex::new(vec![
            medulla::daemon::DaemonRuntime::new(config, run_task, send),
        ])),
        hub_address: "medulla-orchestrator".to_string(),
        env,
        workspace: workspace.to_string(),
        providers: Vec::new(),
        custom_harnesses: Vec::new(),
        router: None,
        attribution: true,
    }
}

/// A server and the client's side of the bridge between them.
fn fixture(workspace: &str) -> (RemoteServer, medulla::bridge::LocalBridge) {
    let network = LocalBridgeNetwork::new();
    let daemon_bridge = network.bind(DAEMON).expect("daemon binds");
    let client_bridge = network.bind(CLIENT).expect("client binds");
    let server = RemoteServer::new(
        shell_sessions(workspace),
        Arc::new(daemon_bridge),
        "test-host".to_string(),
    );
    (server, client_bridge)
}

/// Deliver one client message to the server.
async fn say(server: &mut RemoteServer, body: &str) {
    server.handle(CLIENT, body).await;
}

/// Every remote message the client has been sent so far.
async fn remote_replies(client: &medulla::bridge::LocalBridge) -> Vec<RemoteMessage> {
    client
        .drain_inbox(64)
        .await
        .iter()
        .filter_map(|message| parse_remote_message(&message.text))
        .collect()
}

/// Spin until `check` passes, sampling screens as we go.
///
/// Real children on real ptys are at the mercy of machine load, so the budget is
/// generous: a tight deadline turns "the box was busy" into a red test.
async fn wait_for(
    what: &str,
    server: &mut RemoteServer,
    mut check: impl FnMut(&mut RemoteServer) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if check(server) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out after 30s waiting for: {what}");
}

/// The shell row this host offers.
fn shell_choice(server: &RemoteServer) -> RemoteHarnessChoice {
    match remote_capabilities(server)
        .harnesses
        .into_iter()
        .find(|choice| choice.provider == "shell")
    {
        Some(choice) => choice,
        None => panic!("this host offers no shell"),
    }
}

fn remote_capabilities(server: &RemoteServer) -> medulla::protocol::RemoteCapabilities {
    server.capabilities().clone()
}

/// Open a shell and return its session id.
async fn open_shell(server: &mut RemoteServer, client: &medulla::bridge::LocalBridge) -> String {
    let choice = shell_choice(server);
    say(
        server,
        &encode_remote_message(&RemoteMessage::Open {
            request_id: "r1".to_string(),
            harness: choice,
            workspace: String::new(),
            cols: 80,
            rows: 24,
            name: None,
        }),
    )
    .await;
    for message in remote_replies(client).await {
        if let RemoteMessage::Opened { session_id, .. } = message {
            return session_id;
        }
        if let RemoteMessage::OpenFailed { reason, .. } = message {
            panic!("the shell did not start: {reason}");
        }
    }
    panic!("no answer to the open request");
}

/// The whole of a session's screen, as text.
fn screen_text(server: &RemoteServer, id: &str) -> String {
    let mut out = String::new();
    server.pty().screen_text_into(id, &mut out);
    out
}

#[tokio::test]
async fn hello_is_answered_with_what_this_host_can_do() {
    let (mut server, client) = fixture("/tmp");
    say(
        &mut server,
        &encode_remote_message(&RemoteMessage::Hello {
            client_version: "0.12.0".to_string(),
        }),
    )
    .await;

    let replies = remote_replies(&client).await;
    let capabilities = replies
        .iter()
        .find_map(|message| match message {
            RemoteMessage::Capabilities(capabilities) => Some(capabilities),
            _ => None,
        })
        .expect("hello is answered with capabilities");
    assert_eq!(capabilities.host_name, "test-host");
    assert!(
        capabilities.harnesses.iter().any(|c| c.provider == "shell"),
        "a host with a shell must offer one"
    );
    assert!(
        replies
            .iter()
            .any(|message| matches!(message, RemoteMessage::Sessions { .. })),
        "a client that says hello should learn what is already running"
    );
}

#[tokio::test]
async fn a_client_can_only_start_what_this_host_advertised() {
    // The whole authorization story for opening a session. The choice is
    // resolved by id against what this host offers, never rebuilt from what the
    // client sent — so a client cannot name an arbitrary binary or a provider
    // this machine deliberately does not serve.
    let (mut server, client) = fixture("/tmp");
    say(
        &mut server,
        &encode_remote_message(&RemoteMessage::Open {
            request_id: "r1".to_string(),
            harness: RemoteHarnessChoice {
                id: "shell:/usr/bin/evil".to_string(),
                provider: "shell".to_string(),
                preset: None,
                display_name: "evil".to_string(),
            },
            workspace: String::new(),
            cols: 80,
            rows: 24,
            name: None,
        }),
    )
    .await;

    let refused = remote_replies(&client)
        .await
        .into_iter()
        .any(|message| matches!(message, RemoteMessage::OpenFailed { .. }));
    assert!(refused, "an unadvertised harness must be refused");
    assert!(
        server.pty().rows().is_empty(),
        "nothing may have been started"
    );
}

#[tokio::test]
async fn opening_a_shell_starts_it_and_lists_it() {
    let (mut server, client) = fixture("/tmp");
    let session_id = open_shell(&mut server, &client).await;
    let rows = server.pty().rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, session_id);
    assert_eq!(rows[0].provider, HarnessProvider::Shell);
}

#[tokio::test]
async fn input_reaches_the_pty_and_the_screen_comes_back() {
    // The whole round trip in one test: the client types, the far-side shell
    // runs it, and the output arrives as a frame the client could render.
    let (mut server, client) = fixture("/tmp");
    let session_id = open_shell(&mut server, &client).await;

    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::Subscribe {
            task_id: session_id.clone(),
            max_fps: 10,
            resync: true,
        }),
    )
    .await;
    assert_eq!(server.watching(), 1);

    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::input(
            &session_id,
            b"echo mosh-works-here\n",
        )),
    )
    .await;

    let id = session_id.clone();
    wait_for("the shell to echo the marker", &mut server, |server| {
        screen_text(server, &id).contains("mosh-works-here")
    })
    .await;

    // And the client is actually sent a frame carrying it.
    let mut seen = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !seen {
        server.publish_screens_once().await;
        for message in client.drain_inbox(64).await {
            if let Some(ScreenMessage::Frame(frame)) =
                medulla::protocol::parse_screen_message(&message.text)
            {
                let text: String = frame
                    .rows_changed
                    .iter()
                    .flat_map(|row| row.runs.iter().map(|run| run.text.clone()))
                    .collect();
                if text.contains("mosh-works-here") {
                    seen = true;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        seen,
        "the client was never sent a frame carrying the output"
    );
}

#[tokio::test]
async fn input_is_refused_for_a_session_the_client_is_not_watching() {
    // A subscription is the one thing that says which pane a peer has open, so
    // it is what input is authorized against. Without this check any connected
    // client could type into any session on the host.
    let (mut server, client) = fixture("/tmp");
    let session_id = open_shell(&mut server, &client).await;

    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::input(&session_id, b"echo never-typed\n")),
    )
    .await;

    // Give a real shell every chance to have run it, then assert it did not.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !screen_text(&server, &session_id).contains("never-typed"),
        "input without a subscription must not reach the pty"
    );
}

#[tokio::test]
async fn a_second_subscribe_replaces_the_first() {
    // Channel 1 is one grid per peer, so a client watches one session at a time.
    let (mut server, client) = fixture("/tmp");
    let first = open_shell(&mut server, &client).await;
    let second = open_shell(&mut server, &client).await;
    assert_ne!(first, second);

    for id in [&first, &second] {
        say(
            &mut server,
            &encode_screen_message(&ScreenMessage::Subscribe {
                task_id: id.clone(),
                max_fps: 10,
                resync: true,
            }),
        )
        .await;
    }
    assert_eq!(server.watching(), 1, "one client watches one screen");
    assert_eq!(server.watching_session(CLIENT), Some(second.as_str()));
}

#[tokio::test]
async fn an_unwatched_session_still_appears_in_the_session_list() {
    // The corollary of one-screen-per-client: everything else stays visible in
    // the rail, because the list rides the reliable channel rather than the
    // screen one.
    let (mut server, client) = fixture("/tmp");
    let first = open_shell(&mut server, &client).await;
    let second = open_shell(&mut server, &client).await;
    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::Subscribe {
            task_id: second.clone(),
            max_fps: 10,
            resync: true,
        }),
    )
    .await;

    server.publish_sessions(CLIENT).await;
    let listed: Vec<String> = remote_replies(&client)
        .await
        .into_iter()
        .find_map(|message| match message {
            RemoteMessage::Sessions { rows } => Some(rows.iter().map(|r| r.id.clone()).collect()),
            _ => None,
        })
        .expect("a session list");
    assert!(
        listed.contains(&first),
        "the unwatched session is still listed"
    );
    assert!(listed.contains(&second));
}

#[tokio::test]
async fn closing_a_session_ends_it() {
    let (mut server, client) = fixture("/tmp");
    let session_id = open_shell(&mut server, &client).await;
    say(
        &mut server,
        &encode_remote_message(&RemoteMessage::Close {
            session_id: session_id.clone(),
        }),
    )
    .await;

    let id = session_id.clone();
    wait_for("the shell to exit", &mut server, |server| {
        !matches!(
            server.pty().row(&id).map(|row| row.state),
            Some(crate::worker::pty::PtyState::Running)
        )
    })
    .await;
    assert_eq!(server.watching(), 0);
}

#[tokio::test]
async fn a_resize_reaches_the_emulator() {
    let (mut server, client) = fixture("/tmp");
    let session_id = open_shell(&mut server, &client).await;
    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::Subscribe {
            task_id: session_id.clone(),
            max_fps: 10,
            resync: true,
        }),
    )
    .await;
    say(
        &mut server,
        &encode_screen_message(&ScreenMessage::Resize {
            task_id: session_id.clone(),
            cols: 100,
            rows: 40,
        }),
    )
    .await;

    let snapshot = server
        .pty()
        .screen_rows(&session_id)
        .expect("the session has a screen");
    assert_eq!(snapshot.cells.len(), 40);
    assert_eq!(snapshot.cells[0].len(), 100);
}

#[tokio::test]
async fn an_unknown_body_is_dropped_rather_than_crashing_the_loop() {
    // Inbound bodies are untrusted. A peer sending junk must cost that message
    // and nothing else.
    let (mut server, _client) = fixture("/tmp");
    for body in ["", "not json", "{}", r#"{"kind":"open"}"#] {
        say(&mut server, body).await;
    }
    assert!(server.pty().rows().is_empty());
}

#[test]
fn a_host_launches_agents_with_its_own_configuration() {
    // The point of stage 7, and the part the Docker suite cannot see: a coding
    // agent started from across the network must be launched by the *host's*
    // config, not the client's. Its `[router]` names that machine's keys, its
    // `[[hooks]]` are that machine's commands, and its presets describe binaries
    // that exist over there. Shipping the client's would be both wrong and a way
    // to make one machine run another's commands.
    let mut config = medulla::config::TuiConfig::default();
    config.attribution.commit = false;
    config.router = Some(medulla::config::RouterConfig::default());
    let preset = host_preset();

    let sessions = super::entry::local_sessions(
        &HashMap::new(),
        &config,
        std::slice::from_ref(&preset),
        "/work",
    );

    assert!(
        !sessions.attribution,
        "the host's attribution setting must be the one that applies"
    );
    assert!(
        sessions.router.is_some(),
        "the host's router must reach the launch"
    );
    assert_eq!(
        sessions.custom_harnesses.len(),
        1,
        "a preset defined on the host is offerable to a client that has never heard of it"
    );
    assert_eq!(sessions.custom_harnesses[0].id, "host-only");
    assert_eq!(sessions.workspace, "/work");
}

#[test]
fn a_hosts_own_preset_is_advertised_to_clients() {
    // Follows from the above, and is what the picker actually reads: a preset
    // the host defines has to appear in the capabilities it sends back, or a
    // client could never select it.
    let mut config = medulla::config::TuiConfig::default();
    let preset = host_preset();
    config.attribution.commit = true;
    let sessions = super::entry::local_sessions(
        &HashMap::new(),
        &config,
        std::slice::from_ref(&preset),
        "/work",
    );
    let network = LocalBridgeNetwork::new();
    let server = RemoteServer::new(
        sessions,
        Arc::new(network.bind("daemon-preset").expect("binds")),
        "host".to_string(),
    );
    assert!(
        server
            .capabilities()
            .harnesses
            .iter()
            .any(|choice| choice.id == "host-only"),
        "the host's own preset should be on offer: {:?}",
        server.capabilities().harnesses
    );
}

/// A preset as an operator would have written it on the host's own machine.
///
/// Built through the editor-line parser rather than a struct literal, because
/// that is the only constructor `CustomHarnessConfig` offers — and using it
/// keeps this test honest about what a real config produces.
fn host_preset() -> medulla::config::CustomHarnessConfig {
    medulla::config::CustomHarnessConfig::from_editor_line(
        "host-only | Host Only | claude | some-model |  | this-device",
    )
    .expect("a well-formed preset line")
}
