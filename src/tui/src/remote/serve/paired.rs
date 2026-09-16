//! `medulla daemon <key>` and `medulla daemon --host`: a paired remote host.
//!
//! The mosh-shaped way to add a machine. The Hosts tab mints a key holding
//! everything both ends need to agree on — the client's node id, this host's
//! node id, the pair key and the UDP port — and the operator runs this once
//! with it. From then on the client dials `host:port` directly: no SSH, no
//! backend, and a client that changes networks is picked up from its next
//! authenticated datagram, exactly as the SSH-bootstrapped daemon does.
//!
//! What differs from [`entry`](super::entry) is only the bookkeeping around
//! the same [`RemoteServer`]:
//!
//! - identity comes from the key rather than being minted, so the host's node
//!   id is what the client already expects;
//! - the port is fixed by the key, because the client has to know where to
//!   send before anything has been heard from this side;
//! - there is no connect line and no detaching from SSH — nobody is waiting on
//!   stdout, and the operator's own shell (or service manager) owns the
//!   process;
//! - every pairing on disk is served at once. One TUI is the usual case, but a
//!   second laptop paired later gets its own identity directory, its own port
//!   and its own server in this same process.
//!
//! Keeping the daemon running is the operator's job: this prints how, and does
//! not install a service itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use medulla::bridge::{LinkBridge, LinkBridgeConfig, LinkPeer};
use medulla_link::keys::{self, ForwarderKey, HostKey, NodeId, NodeState, Role};
use medulla_link::{Link, LinkConfig, LinkPath, PeerConfig};

use super::entry::{flag, hostname_of, local_sessions, CLIENT_ADDRESS};
use super::{bind_address, client_dir, control, link_dir, RemoteServer, WorkflowHost};

/// What a pairing leaves on disk beside its link identity, so `--host` can
/// bring it back up without the key.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pairing {
    /// The client this pairing serves.
    pub client: NodeId,
    /// The UDP port to listen on.
    pub port: u16,
}

/// The pairing record's file name inside a client's directory.
pub const PAIRING_FILE: &str = "pairing.json";

/// Run the paired host until told otherwise.
///
/// With a key: record the pairing, then re-exec into `--host` mode so the key
/// never lingers in this process's argv (see [`reexec_without_key`]). With
/// `--host` alone: serve every pairing on disk. Either way the process stays
/// up until a signal.
///
/// # Errors
///
/// When the key does not decode, when neither a key nor a pairing on disk is
/// available, when an identity cannot be written or locked, when a port
/// cannot be bound, or when re-exec fails — each fatal, because a client
/// would wait forever on a host that half-started.
pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let env: HashMap<String, String> = std::env::vars().collect();
    if let Some(path) = flag(args, "--config") {
        std::env::set_var(medulla::config::CONFIG_PATH_ENV, path);
    }

    // A pasted key is only ever needed once, to write the pairing to disk.
    // `ps`/`/proc/<pid>/cmdline` show a process's argv for its whole
    // lifetime, not just at startup, so a daemon that kept serving with the
    // key still on its command line would leak it — the shared secret and
    // both node ids — to every other user on a multi-user host for as long
    // as it ran. Pair, then hand off to a fresh process invoked with
    // `--host` instead, which re-derives everything from the pairing this
    // one just wrote.
    if let Some(text) = args.iter().find(|arg| HostKey::looks_like(arg)) {
        let key = HostKey::decode(text)?;
        let home = medulla::home::medulla_home(&env);
        pair(&home, &key)?;
        eprintln!(
            "medulla daemon: paired with client {} on udp/{}",
            key.client, key.port
        );
        return reexec_without_key(args, text);
    }

    run_in(&env, args).await
}

/// Re-invoke this binary with the pairing key removed from argv (or, on a
/// platform without process replacement, spawn an equivalent child and exit
/// this process) so the key does not remain visible in this process's
/// command line for as long as the daemon serves.
///
/// The caller must have already written the pairing to disk: the
/// re-invocation always serves via `--host`, which reads every pairing found
/// there rather than expecting another key.
///
/// # Errors
///
/// When the current executable's path cannot be determined, or — on a
/// platform without `exec` — when the child process cannot be spawned or
/// exits with a signal rather than a code.
fn reexec_without_key(args: &[String], key_arg: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let new_args = reexec_args(args, key_arg);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // `exec` replaces this process's image (and its argv) in place;
        // on success it never returns.
        let error = std::process::Command::new(&exe).args(&new_args).exec();
        anyhow::bail!(
            "failed to re-exec {} without the pairing key: {error}",
            exe.display()
        );
    }

    #[cfg(not(unix))]
    {
        // No `exec` here, so fall back to spawn-and-exit. `spawn` — not
        // `status`, which would block this process on the child and defeat
        // the whole point: the child inherits stdio and keeps running
        // independently once spawned, so this process can exit immediately
        // rather than sitting around with the key still on its own argv for
        // as long as the daemon serves.
        std::process::Command::new(&exe).args(&new_args).spawn()?;
        std::process::exit(0);
    }
}

/// Build the argv for [`reexec_without_key`]: `args` is what [`run`] was
/// given — everything after the binary name, `daemon` already stripped by
/// `main`'s own dispatch — so this restores `daemon`, drops `key_arg`, and
/// makes sure `--host` is present so the re-invocation reads pairings from
/// disk instead of expecting another key.
pub(crate) fn reexec_args(args: &[String], key_arg: &str) -> Vec<String> {
    let mut out: Vec<String> = vec!["daemon".to_string()];
    out.extend(args.iter().filter(|arg| arg.as_str() != key_arg).cloned());
    if !out.iter().any(|arg| arg == "--host") {
        out.push("--host".to_string());
    }
    out
}

/// [`run`] over an explicit environment.
///
/// The seam a test uses to stand a paired host up under a scratch home
/// without touching the process environment; `MEDULLA_HOME` in `env` is
/// where the pairing is read from and written to.
///
/// # Errors
///
/// As [`run`].
pub async fn run_in(env: &HashMap<String, String>, args: &[String]) -> anyhow::Result<()> {
    let env = env.clone();
    let env = &env;
    let home = medulla::home::medulla_home(env);
    let explicit_config = flag(args, "--config");
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());
    let workspace = flag(args, "--workspace")
        .map(|dir| {
            std::fs::canonicalize(&dir)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or(dir)
        })
        .unwrap_or_else(|| cwd.clone());
    let loaded = medulla::config::load_config(explicit_config.as_deref(), env, Path::new(&cwd))?;

    // Pair first, so the key's pairing is on disk before anything is served
    // and a `--host` re-run finds it.
    let pasted = args.iter().find(|arg| HostKey::looks_like(arg));
    let mut pairings = Vec::new();
    if let Some(text) = pasted {
        let key = HostKey::decode(text)?;
        let pairing = pair(&home, &key)?;
        eprintln!(
            "medulla daemon: paired with client {} on udp/{}",
            key.client, key.port
        );
        pairings.push(pairing);
    } else {
        pairings = pairings_on_disk(&home);
        if pairings.is_empty() {
            anyhow::bail!(
                "no pairing on disk — run `medulla daemon <key>` with the key the Hosts tab shows"
            );
        }
    }

    // One control plane for the process: `attach_mcp` reads it process-wide,
    // and every session any server here opens is launched from this process.
    let first = client_dir(&home, pairings[0].client);
    let _control = control::start(env, &loaded.config, &first, |line| eprintln!("{line}")).await;
    let presets =
        medulla::config::load_layered_custom_harnesses(&loaded.sources).unwrap_or_default();
    let host_name = flag(args, "--host-name")
        .unwrap_or_else(|| hostname_of(env).unwrap_or_else(|| "remote host".to_string()));
    let workspaces: Vec<String> = flag(args, "--workspaces")
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|dir| !dir.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let mut servers = Vec::new();
    for pairing in &pairings {
        let dir = link_dir(&client_dir(&home, pairing.client));
        // A missing or malformed identity for one pairing (its `pairing.json`
        // survived, but `link/node.json` did not) must not take every other,
        // healthy pairing offline with it — the same "skip, and only fail
        // outright when nothing could be served" rule the bind failure below
        // already follows.
        let state = keys::read_node_state(&keys::node_path(&dir))?;
        let pair_key = state
            .peers
            .iter()
            .find(|peer| peer.node_id == pairing.client)
            .map(|peer| peer.pair_key.clone())
            .ok_or_else(|| anyhow::anyhow!("pairing for client {} has no key material; pair it again", pairing.client))?;
        let mut config = LinkConfig::new(&dir);
        config.bind = bind_address(pairing.port);
        config.path = LinkPath::Direct {
            endpoint: super::unrouted_placeholder(config.bind),
        };
        config.peers = vec![PeerConfig {
            node_id: pairing.client,
            pair_key,
        }];
        // With several pairings on disk, one whose port cannot be bound must
        // not take the others down: it is reported and skipped, and the
        // process still fails when *nothing* could be served.
        let link = match Link::connect(config).await {
            Ok(link) => link,
            Err(error) => {
                let message = format!(
                    "could not listen on udp/{} for client {} ({error}); if the port is taken, \
                     re-issue the key from the Hosts tab with another port",
                    pairing.port, pairing.client
                );
                if pairings.len() == 1 {
                    anyhow::bail!(message);
                }
                eprintln!("medulla daemon: {message}");
                continue;
            }
        };
        let bridge = Arc::new(LinkBridge::new(
            Arc::new(link),
            LinkBridgeConfig {
                node_name: "medulla-host".to_string(),
                peers: vec![LinkPeer {
                    name: CLIENT_ADDRESS.to_string(),
                    node_id: pairing.client,
                }],
            },
        ));
        let sessions = local_sessions(env, &loaded.config, &presets, &workspace);
        let workflows = workflow_host(env, &loaded, Path::new(&cwd));
        let mut server = RemoteServer::new(sessions, bridge, host_name.clone())
            .with_workspaces(workspaces.clone())
            .with_log(Arc::new(|line: &str| eprintln!("{line}")));
        if let Some(workflows) = workflows {
            server = server.with_workflows(workflows);
        }
        eprintln!(
            "medulla daemon: serving client {} on udp/{} (workspace: {workspace})",
            pairing.client, pairing.port
        );
        servers.push(server);
    }

    if servers.is_empty() {
        anyhow::bail!("no pairing could be served");
    }
    eprintln!(
        "medulla daemon: this machine is paired. Keep this process running to stay reachable — \
         for example `systemd-run --user --unit medulla-host medulla daemon --host`, a launchd \
         job, or a tmux window."
    );

    let mut handles = Vec::new();
    for mut server in servers {
        handles.push(tokio::spawn(async move { server.run().await }));
    }
    for handle in handles {
        let _ = handle.await;
    }
    Ok(())
}

/// Record `key`'s pairing under this home, minting the host identity it names.
///
/// Re-running with the same key is a no-op — the identity is already the one
/// the client expects. A *different* key for the same client replaces the
/// pairing, which is what re-issuing from the Hosts tab means.
///
/// # Errors
///
/// When the directory cannot be created or the identity cannot be written or
/// locked — the last meaning a daemon for this client is already running, and
/// the operator should stop it before re-pairing.
pub fn pair(home: &Path, key: &HostKey) -> anyhow::Result<Pairing> {
    // Each client's link is its own socket, so two clients cannot share a
    // port on this host. Two TUIs that each picked the first free port in
    // *their* config would collide here, and the collision is only visible on
    // this side — so it is refused now, where the operator is looking, rather
    // than on the next `--host` restart.
    if let Some(taken) = pairings_on_disk(home)
        .into_iter()
        .find(|pairing| pairing.port == key.port && pairing.client != key.client)
    {
        anyhow::bail!(
            "udp/{} is already used by this host's pairing with client {}; re-issue the key \
             from the Hosts tab with another port",
            key.port,
            taken.client
        );
    }
    let dir = client_dir(home, key.client);
    let link = link_dir(&dir);
    std::fs::create_dir_all(&link)?;
    // Owner-only, whatever the umask: the control socket for this client's
    // sessions is bound in here, and `control::start` refuses a directory
    // other users could replace it in — so a permissive umask on the host
    // would otherwise cost every remote coding agent its tools.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&dir, &link] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let path = keys::node_path(&link);
    let fresh = || NodeState {
        version: 2,
        node_id: key.host,
        role: Role::Host,
        pair_key: key.pair_key.clone(),
        // Never used on the direct path; present because the schema has it.
        forwarder_key: ForwarderKey::generate(),
        forwarder_endpoint: String::new(),
        peer_node_id: key.client,
        peers: vec![keys::EnrolledPeer {
            node_id: key.client,
            pair_key: key.pair_key.clone(),
        }],
        seq_reservation: 1,
    };
    let existing = keys::read_node_state(&path).ok();
    let matches = existing.as_ref().is_some_and(|state| {
        state.node_id == key.host
            && state
                .peers
                .iter()
                .any(|peer| peer.node_id == key.client && peer.pair_key == key.pair_key)
    });
    if !matches {
        // Take the lock for the write: a daemon already serving this client
        // holds it, and rewriting its key underneath it would leave the two
        // ends disagreeing.
        let _lock = keys::lock_node_dir(&link).map_err(|error| {
            anyhow::anyhow!(
                "a daemon for client {} is already running ({error}); stop it before pairing again",
                key.client
            )
        })?;
        keys::write_node_state(&path, &fresh())?;
    }
    let pairing = Pairing {
        client: key.client,
        port: key.port,
    };
    std::fs::write(
        dir.join(PAIRING_FILE),
        serde_json::to_string_pretty(&pairing)?,
    )?;
    Ok(pairing)
}

/// Every pairing recorded under `home`, in directory order.
pub fn pairings_on_disk(home: &Path) -> Vec<Pairing> {
    let root = home.join("remote").join("hosts");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Pairing)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path().join(PAIRING_FILE);
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, serde_json::from_str(&text).ok()?))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, pairing)| pairing).collect()
}

/// The workflow surface this host serves, when the build has one.
#[cfg(feature = "workflows")]
fn workflow_host(
    env: &HashMap<String, String>,
    loaded: &medulla::config::LoadedConfig,
    cwd: &Path,
) -> Option<WorkflowHost> {
    Some(WorkflowHost::from_config(env, loaded.clone(), cwd))
}

/// A slim build serves sessions and answers every workflow call with a reason.
#[cfg(not(feature = "workflows"))]
fn workflow_host(
    _env: &HashMap<String, String>,
    _loaded: &medulla::config::LoadedConfig,
    _cwd: &Path,
) -> Option<WorkflowHost> {
    Some(WorkflowHost)
}
