//! `medulla daemon --direct`: bring a remote host up and serve one client.
//!
//! The whole sequence, in order, because each step depends on the one before:
//!
//! 1. Read `--peer-node`, the client's node id. It is public — node ids ride in
//!    cleartext headers by design — so it is the one half of the pairing that
//!    can safely travel in argv.
//! 2. Mint or open this host's identity and enroll that client under a fresh
//!    pair key ([`enroll_client`]).
//! 3. Bind a UDP socket and bring up a [`LinkPath::Direct`] link.
//! 4. Print the connect line to stdout, where the client's SSH session reads it,
//!    and flush — SSH may close the moment it has the line.
//! 5. Serve until told otherwise.
//!
//! Nothing here contacts the backend, and nothing requires the host to have been
//! enrolled or to have a coding CLI installed. A machine with `/bin/sh` and this
//! binary is a usable remote host, which is the point.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use medulla::bridge::{LinkBridge, LinkBridgeConfig, LinkPeer};
use medulla_link::keys::NodeId;
use medulla_link::{Link, LinkConfig, LinkPath, PeerConfig};

use super::{bind_address, enroll_client, remote_link_dir, ConnectLine, RemoteServer};
use crate::ui::harness_pane::LocalSessions;
use crate::worker::pty::PtyManager;

/// The bridge address the client is known by on this side.
///
/// A constant because a direct link has exactly one peer: there is nobody else
/// to tell apart, and a name derived from the node id would only make log lines
/// harder to read.
pub const CLIENT_ADDRESS: &str = "medulla-client";

/// Run the direct-serving daemon.
///
/// # Errors
///
/// When `--peer-node` is missing or unparseable, when the identity cannot be
/// minted or locked, or when the UDP socket cannot be bound. Each is fatal:
/// there is no degraded mode where a client could still reach this process.
pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let peer = flag(args, "--peer-node")
        .and_then(|value| NodeId::from_hex(&value))
        .ok_or_else(|| {
            anyhow::anyhow!("--peer-node <32 hex> is required; the client supplies its node id")
        })?;
    let port: u16 = flag(args, "--port")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let env: HashMap<String, String> = std::env::vars().collect();
    let home = medulla::home::medulla_home(&env);
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());
    // Where sessions this host serves will run. The single most consequential
    // argument, exactly as it is for `daemon --tui`: a harness serving a client
    // edits files here, so defaulting to the launch directory means the shell
    // that started the daemon decides what the client can touch.
    let workspace = flag(args, "--workspace")
        .map(|dir| {
            std::fs::canonicalize(&dir)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or(dir)
        })
        .unwrap_or(cwd);

    let dir = remote_link_dir(&home);
    let (node_id, pair_key) = enroll_client(&dir, peer)?;

    let mut config = LinkConfig::new(&dir);
    config.bind = bind_address(port);
    // The client's address is not known yet and does not need to be: it dials in,
    // and the link learns where it is from the first datagram that authenticates
    // (see `medulla_link::link::direct`). An unroutable placeholder is honest
    // about that — nothing is ever sent there, because nothing is sent until the
    // client has been heard from.
    config.path = LinkPath::Direct {
        endpoint: "127.0.0.1:9".to_string(),
    };
    config.peers = vec![PeerConfig {
        node_id: peer,
        pair_key: pair_key.clone(),
    }];
    let link = Link::connect(config).await?;
    let bound = link.local_addr();

    let bridge = Arc::new(LinkBridge::new(
        Arc::new(link),
        LinkBridgeConfig {
            node_name: "medulla-host".to_string(),
            peers: vec![LinkPeer {
                name: CLIENT_ADDRESS.to_string(),
                node_id: peer,
            }],
        },
    ));

    let sessions = local_sessions(&env, &workspace);
    let host_name = flag(args, "--host-name").unwrap_or_else(|| {
        hostname_of(&env).unwrap_or_else(|| "remote host".to_string())
    });
    let workspaces = flag(args, "--workspaces")
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|dir| !dir.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // Printed and flushed before serving: the client is blocked on this line, and
    // SSH may close the moment it has it.
    let line = ConnectLine {
        port: bound.port(),
        node_id,
        pair_key,
    };
    println!("{}", line.render());
    std::io::stdout().flush()?;

    let mut server = RemoteServer::new(sessions, bridge, host_name).with_workspaces(workspaces);
    server.run().await;
    Ok(())
}

/// Build the session factory this host serves with.
///
/// Deliberately the same `LocalSessions` the operator's own picker uses, so a
/// session opened from across the network is launched by exactly the code that
/// launches one opened here — MCP registration, managed skills, router injection
/// and attribution included for a coding agent, and deliberately none of them
/// for a shell.
fn local_sessions(env: &HashMap<String, String>, workspace: &str) -> LocalSessions {
    // `None` for the filter: a remote host offers whatever it actually has,
    // rather than a list configured on the client's machine.
    let providers = medulla::daemon::providers::detect_providers(env, None, None);
    let config = medulla::daemon::DaemonConfig {
        hooks: medulla::harness_hooks::HooksConfig::default(),
        providers: providers.clone(),
        default_provider: providers
            .first()
            .copied()
            .unwrap_or(medulla::protocol::HarnessProvider::Shell),
        workspace: workspace.to_string(),
        accessible_dirs: Vec::new(),
        env: env.clone(),
        task_timeout_ms: 600_000,
        capability_timeout_ms: None,
        concurrency: 4,
        status_throttle_ms: 1_000,
        max_pending: 8,
        model: None,
        agent: None,
        extra_args: Vec::new(),
        skip_permissions: false,
        router: None,
        custom_harnesses: Vec::new(),
        budget: None,
        attribution: true,
    };
    let run_task: medulla::daemon::providers::RunTaskFn = Arc::new(|_| {
        Box::pin(async {
            Err("this host serves operator sessions, not dispatched tasks".to_string())
        })
    });
    let send: medulla::daemon::SendFn = Arc::new(|_, _| Box::pin(async {}));
    LocalSessions {
        hooks: medulla::harness_hooks::HooksConfig::default(),
        log: None,
        sessions: PtyManager::new(),
        runtimes: Arc::new(std::sync::Mutex::new(vec![
            medulla::daemon::DaemonRuntime::new(config, run_task, send),
        ])),
        hub_address: "medulla-orchestrator".to_string(),
        env: env.clone(),
        workspace: workspace.to_string(),
        providers,
        custom_harnesses: Vec::new(),
        router: None,
        attribution: true,
    }
}

/// This machine's name, for the client's rail.
fn hostname_of(env: &HashMap<String, String>) -> Option<String> {
    env.get("HOSTNAME")
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
        })
}

/// Read `--name value` or `--name=value`.
fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == name {
            return it.next().cloned().filter(|value| !value.is_empty());
        }
        if let Some(rest) = arg.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return (!rest.is_empty()).then(|| rest.to_string());
        }
    }
    None
}
