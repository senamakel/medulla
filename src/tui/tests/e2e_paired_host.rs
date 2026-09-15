//! A paired remote host, end to end over real UDP on the loopback: the key
//! the Hosts tab mints, the daemon `medulla daemon <key>` stands up from it,
//! and the client that dials the port the key named — with a shell session
//! opened across it and a workflow call answered by the host's own store.
//!
//! No SSH and no backend anywhere in the chain, which is the whole point of
//! pairing this way. The daemon is the real `paired::run_in` under a scratch
//! home; the client is the real `RemoteHost::dial`.

#![cfg(unix)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use medulla::config::RemoteHostSection;
use medulla_link::keys::HostKey;
use medulla_tui::remote::client::pairing::{mint_pairing, pairing_command};
use medulla_tui::remote::client::{PumpEvent, RemoteHost};
use medulla_tui::remote::serve::paired;

/// A UDP port nothing is listening on right now.
fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Pump `host` until `done` says so, or the deadline passes.
async fn pump_until(
    host: &mut RemoteHost,
    what: &str,
    mut done: impl FnMut(&[PumpEvent], &RemoteHost) -> bool,
) -> Vec<PumpEvent> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        seen.extend(host.pump().await);
        if done(&seen, host) {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}; saw {seen:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pasted_key_pairs_a_host_the_client_can_dial_and_use() {
    let client_home = tempfile::tempdir().unwrap();
    let host_home = tempfile::tempdir().unwrap();
    let port = free_port();

    // The Hosts tab's half: mint the key for this device.
    let state_dir = medulla_tui::remote::client::task::client_state_dir(client_home.path(), "box");
    let key = mint_pairing(&state_dir, port).unwrap();
    let command = pairing_command(&key);
    let pasted = command.strip_prefix("medulla daemon ").unwrap().to_string();
    assert_eq!(
        HostKey::decode(&pasted).unwrap(),
        key,
        "the command carries the key intact"
    );

    // The host's half: `medulla daemon <key>` under its own home.
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert(
        "MEDULLA_HOME".to_string(),
        host_home.path().to_string_lossy().into_owned(),
    );
    if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".to_string(), path);
    }
    env.insert("MEDULLA_SHELL_BIN".to_string(), "/bin/sh".to_string());
    env.insert("HOSTNAME".to_string(), "boxy".to_string());
    let workspace = host_home.path().to_string_lossy().into_owned();
    let args = vec![pasted.clone(), "--workspace".to_string(), workspace];
    let daemon = tokio::spawn(async move { paired::run_in(&env, &args).await });

    // The pairing lands on disk before the daemon serves, so a `--host` re-run
    // would find it.
    let deadline = Instant::now() + Duration::from_secs(10);
    let host_root = medulla::home::medulla_home(&HashMap::from([(
        "MEDULLA_HOME".to_string(),
        host_home.path().to_string_lossy().into_owned(),
    )]));
    while paired::pairings_on_disk(&host_root).is_empty() {
        assert!(Instant::now() < deadline, "the pairing was never recorded");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(paired::pairings_on_disk(&host_root)[0].port, port);

    // The client's half: dial the port the key named.
    let section = RemoteHostSection {
        id: "box".to_string(),
        host: "127.0.0.1".to_string(),
        udp_port: port,
        enabled: true,
        ..RemoteHostSection::default()
    };
    let mut client = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.is_none() {
        match RemoteHost::dial("box", &section, &state_dir).await {
            Ok(host) => client = Some(host),
            Err(error) => {
                assert!(Instant::now() < deadline, "could not dial: {error}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    let mut client = client.unwrap();
    client.hello().await.unwrap();
    pump_until(&mut client, "the host to answer hello", |events, _| {
        events.iter().any(|event| *event == PumpEvent::Connected)
    })
    .await;
    assert_eq!(client.capabilities().host_name, "boxy");
    let shell = client
        .capabilities()
        .harnesses
        .iter()
        .find(|choice| choice.provider == "shell")
        .expect("a paired host offers a shell")
        .id
        .clone();

    // A session across the link, and its screen.
    client
        .open("r1", &shell, "", 80, 24, Some("probe".to_string()))
        .await
        .unwrap();
    let events = pump_until(&mut client, "the session to open", |events, _| {
        events
            .iter()
            .any(|event| matches!(event, PumpEvent::Opened(_)))
    })
    .await;
    let session_id = events
        .iter()
        .find_map(|event| match event {
            PumpEvent::Opened(id) => Some(id.clone()),
            _ => None,
        })
        .unwrap();
    client.watch(&session_id).await.unwrap();
    client.input(b"echo paired-ok\n").await.unwrap();
    pump_until(&mut client, "the shell's output", |_, host| {
        host.screen().is_some_and(|screen| {
            screen.cells.iter().any(|row| {
                row.iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<String>()
                    .contains("paired-ok")
            })
        })
    })
    .await;

    // The host's workflow store, over the same link: empty, and answered.
    client
        .workflow_call("q1", "store_list", serde_json::json!({}))
        .await
        .unwrap();
    let events = pump_until(&mut client, "the workflow answer", |events, _| {
        events
            .iter()
            .any(|event| matches!(event, PumpEvent::WorkflowResult { .. }))
    })
    .await;
    let answer = events
        .iter()
        .find_map(|event| match event {
            PumpEvent::WorkflowResult { request_id, result } if request_id == "q1" => {
                Some(result.clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(answer, Ok(serde_json::json!([])));

    client.close(&session_id).await.unwrap();
    daemon.abort();
}
