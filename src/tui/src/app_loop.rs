//! TUI startup: config load, runtime selection, terminal setup, the optional
//! pre-app login screen, and background-service wiring before handing off to the
//! [`crate::event_loop::run`] loop.
//!
//! [`run_tui`] selects a runtime — the embedded OpenHuman core when compiled
//! in, otherwise the pre-cutover backend-token → login-screen → mock chain —
//! installs the panic-safe terminal guard, starts
//! the optional tiny.place presence service, runs the event loop, and tears
//! everything down on exit.

use std::io::{self, IsTerminal};
use std::sync::{Arc, Mutex};

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use medulla::config::load_config;
use medulla::runtime::mock::MockRuntime;
use medulla::runtime::Runtime;
use medulla_tui::cli::parse_tui_args;

use crate::event_loop::{run, SessionWiring};
use crate::terminal::{restore, TermGuard};

/// Parse TUI args, select a runtime, set up the terminal, optionally run the
/// login screen, start background services, and drive the event loop to exit.
pub(crate) async fn run_tui(raw: &[String]) -> anyhow::Result<()> {
    let args = parse_tui_args(raw);

    if !io::stdout().is_terminal() {
        eprintln!("medulla-tui requires an interactive terminal (TTY).");
        std::process::exit(1);
    }

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let loaded = load_config(args.config.as_deref(), &env, &cwd)?;
    let home = medulla::home::medulla_home(&env);

    // Bind the embedded core's state directory to this process's Medulla home
    // BEFORE anything can construct the core. Both resolve state independently
    // otherwise, which would silently route memory, flows, and credentials into
    // the developer's real `~/.openhuman` even on a `MEDULLA_HOME=$(mktemp -d)`
    // scratch run — the recipe that exists precisely to avoid that. Cheap, and
    // a no-op when the operator set `OPENHUMAN_WORKSPACE` themselves.
    #[cfg(feature = "openhuman-core")]
    medulla::core_host::bind_workspace(&env, &home);

    // Runtime selection.
    //
    // Built with `openhuman-core`, the embedded core is THE runtime: there is
    // no token lookup, no login screen, and no mock fallback, because there is
    // nothing to fall back from — the core runs in this process. `--mock` is
    // still honoured ahead of it, since that is an explicit operator request
    // for the offline demo rather than a fallback.
    //
    let mut runtime: Option<Arc<dyn Runtime>> = None;
    let mut startup_status: Option<String> = None;

    // Shared hub roster slot: filled after the hub connects, read by the
    // runtime's worker surface so the Workers tab manages the hub's tiny.place
    // peers live.
    let hub_slot: crate::hub_relay::HubSlot = Arc::new(Mutex::new(None));
    // Active workspace roots whose `MEDULLA.md` profiles ride every backend
    // session mint (`workspaceProfiles`). Roots without a profile are skipped by
    // the collector, so passing every configured workspace is safe.
    let workspace_roots: Vec<std::path::PathBuf> = loaded
        .config
        .workflow
        .workspaces
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    // The agent's read/write root. OpenHuman defaults this to
    // `~/OpenHuman/projects`, which a Medulla operator has never used — their
    // repos are the configured workspace roots. Binding the first keeps the
    // agent writing where the operator actually works. Also non-overriding.
    #[cfg(feature = "openhuman-core")]
    medulla::core_host::bind_action_dir(&env, workspace_roots.first().map(|p| p.as_path()));
    // The hub narrates itself; those lines must not reach the terminal while the
    // TUI owns the screen, so they are captured here instead.
    let hub_logs = medulla_tui::log::LogBuffer::new();
    // Persist them too: the failures worth chasing are usually noticed after the
    // fact, and an in-memory ring dies with the process.
    let log_dir = medulla_tui::log::default_log_dir(&env);
    // Held apart rather than written into `startup_status`: this runs before
    // anything else could have set one, so `get_or_insert` always won here and
    // was then overwritten by every later assignment — the line never showed on
    // any path that reported anything at all. It is the least interesting thing
    // that could be said at startup, so it belongs at the end of the fallback
    // chain, not the front.
    let log_note = hub_logs
        .attach_file(&log_dir, "orchestrator")
        .map(|path| format!("logging to {}", path.display()));

    // Persona-memory service (tinycortex), on by default. Wired into the app
    // itself, which reads it for the Memory tab, so memory works on the backend
    // and mock paths alike.
    let memory_settings = medulla::memory::env::resolve_with_backend(
        loaded.config.memory.as_ref(),
        &loaded.config.backend,
        &env,
        &medulla::home::medulla_home(&env),
    );
    let memory_service: Option<Arc<medulla::memory::MemoryService>> = if memory_settings.enabled {
        match medulla::memory::MemoryService::open(memory_settings) {
            Ok(svc) => Some(Arc::new(svc)),
            Err(e) => {
                startup_status = Some(format!("memory service failed to open ({e})"));
                None
            }
        }
    } else {
        None
    };

    if args.mock {
        // Explicit offline demo: skip the token lookup and the login screen
        // entirely so the TUI is drivable with no backend at all.
        runtime = Some(Arc::new(MockRuntime::demo()));
        startup_status = Some("running the offline mock runtime (--mock)".to_string());
    }
    // The embedded core, whenever it is compiled in. Deliberately ahead of the
    // old token/login chain: a host that ships the core has no reason to dial a
    // remote backend, and keeping that chain as a general fallback would mean a
    // misconfiguration silently downgrades to a different runtime with
    // different behaviour instead of surfacing itself.
    //
    // The one exception is a core that booted but has no Medulla backend to
    // talk to — no configured URL, or nobody signed in. That is not a
    // misconfiguration to surface, it is the documented credential-free start,
    // and every drive method would otherwise return the same error behind a UI
    // that looks live. It takes the offline demo, exactly as `--mock` does.
    #[cfg(feature = "openhuman-core")]
    if runtime.is_none() {
        match medulla::core_host::boot().await {
            Ok(core) => 'core: {
                if let medulla::core_host::Readiness::Unconfigured(why) =
                    medulla::core_host::probe_medulla(&core).await
                {
                    runtime = Some(Arc::new(MockRuntime::demo()));
                    startup_status = Some(format!("{why} — running the offline mock runtime"));
                    break 'core;
                }
                let rt = medulla::runtime::openhuman::OpenHumanRuntime::new(Arc::new(core));
                // First fetch before the UI paints, so the initial frame shows
                // real state rather than an empty one that fills in a beat later.
                rt.refresh().await;
                let rt = Arc::new(rt);
                // Start replaying events before the UI paints. Without this a
                // submitted turn is accepted and nothing ever returns to the
                // transcript, which reads as a hang rather than a missing loop.
                rt.spawn_poll_loop();
                runtime = Some(rt);
            }
            Err(e) => {
                // Boot failure is fatal rather than a downgrade. The core is
                // in-process, so a failure here means a broken workspace or
                // config — conditions the operator must see and fix, not have
                // papered over by a mock that then behaves differently.
                //
                // No terminal teardown needed: this runs before `TermGuard`
                // takes over the screen, so the error reaches a normal stdout.
                anyhow::bail!("failed to start the embedded OpenHuman core: {e}");
            }
        }
    }

    // Restore the terminal on panic before the default hook prints the message.
    let alt = args.alt_screen;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(alt, true);
        default_hook(info);
    }));

    let guard = TermGuard::setup(args.alt_screen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let runtime = runtime.expect("a runtime is always selected");

    // First-run welcome: offer promotional credit for sharing coding-agent
    // history. Gated locally by `[onboarding] welcomeCompleted` so a returning
    // user is never re-prompted; the backend independently refuses a second
    // grant. Only runs against a real authenticated backend — never on the mock.
    let home_config_path = home.join("config.toml");
    // Every write-back (onboarding flag, routing strategy, …) must target the
    // file whose value *wins on the next launch*, or the change is silently lost —
    // the welcome flow reappears, the saved strategy reverts. That target is:
    //   1. the explicit --config file when one was passed (discovery is bypassed);
    //   2. otherwise the highest-precedence file that actually contributed to the
    //      layered load (`sources` is ordered low → high, so `.last()`), which is
    //      the project-local `.medulla/config.toml` / `medulla.toml` when present;
    //   3. otherwise the home config (nothing was discovered to layer).
    // The welcome/credit-sharing flow ran only against an authenticated cloud
    // backend, which the embedded core replaces; it returns with the auth
    // migration rather than being reconstructed against a core that has no
    // notion of a Medulla account.
    let sharing = None;
    let active_config_path = args
        .config
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| loaded.sources.last().map(std::path::PathBuf::from))
        .unwrap_or_else(|| home_config_path.clone());

    // Optional background tiny.place presence service (observational only): keep
    // the identity online, auto-accept peer contacts, and poll peer presence,
    // surfacing all of it into the Overview panel and Agents lanes.
    let mut tinyplace_status: Option<String> = None;
    let tinyplace_service = match &loaded.config.tinyplace {
        Some(tp) => match medulla::tinyplace::service::TinyplaceService::start(tp) {
            Ok(service) => Some(service),
            Err(e) => {
                tinyplace_status = Some(format!("tinyplace service failed to start ({e})"));
                None
            }
        },
        None => None,
    };
    let tinyplace_obs = tinyplace_service.as_ref().map(|s| s.observation());

    // Backend runtime only: start the orchestrator hub so the hosted brain's
    // delegated tasks reach local tiny.place workers, and fill the roster slot so
    // the Workers tab manages it live. Opt-in via `MEDULLA_TINYPLACE_PEER` /
    // `MEDULLA_HUB_WORKERS`; the session is dropped (disconnected) on exit.
    //
    // The hub is scoped to the *authenticated* session: its Socket.IO uplink
    // carries the current account's JWT and its roster handle is that account's.
    // On a relogin (below) it is torn down and re-started for the new account so
    // no worker mutation or task relay ever targets a revoked/stale session.
    // This device also *runs* the work, unless `[host].enabled = false` /
    // `MEDULLA_HOST=0`. The host binds an address on a bus the hub dispatches
    // over, so a task for this machine is delivered in-process — no relay, no
    // second identity, no contact edge between two programs on one laptop.
    //
    // Started before the hub because the hub advertises it: the roster it
    // registers with the backend has to name this host from the first moment, or
    // the orchestrator's opening move has nowhere to send work.
    let local_network = medulla::bridge::LocalBridgeNetwork::new();
    // A bad `[host]` section is reported exactly like a failed start: this
    // machine does not host, and the operator is told why. `and_then` keeps the
    // two failure kinds — unparseable config, unstartable host — on one path.
    let local_host = match crate::local_host::options_from_config(
        &loaded.config.host,
        &env,
        loaded.config.router.clone(),
        loaded.config.budget.clone(),
        Some(hub_logs.sink()),
    )
    .and_then(|options| {
        crate::local_host::start(&loaded.config.host, &env, &local_network, options)
    }) {
        Ok(host) => host,
        Err(e) => {
            // Not fatal: the orchestrator still drives remote workers. But it is
            // the difference between "nothing happens" and "nothing happens
            // *here*", so it goes on the status line rather than only the log.
            hub_logs.push(format!("host: not hosting on this device ({e})"));
            startup_status.get_or_insert(format!("not hosting on this device ({e})"));
            None
        }
    };
    if let Some(host) = &local_host {
        hub_logs.push(format!(
            "host: serving [{}] as {} in {}",
            host.providers()
                .iter()
                .map(|provider| provider.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            host.address(),
            host.workspace()
        ));
    }
    let local_dispatch = crate::hub_relay::LocalDispatch {
        network: local_network,
        hub_address: medulla::hub::DEFAULT_LOCAL_HUB_ADDRESS.to_string(),
        // Always known, even with hosting off — it is what identifies a
        // remembered local roster entry that must not be inherited.
        host_address: crate::local_host::host_address(&loaded.config.host),
        host: local_host.as_ref().map(|host| host.spec().clone()),
    };

    // Start the hub unconditionally. It used to be gated on an authenticated
    // cloud client; it reads its own credentials from the Medulla home and
    // returns `None` when there are none, so the gate only duplicated a check it
    // already makes. The hub is tiny.place/harness wiring and stays TUI-side
    // regardless of which runtime backs the session.
    let _hub_session = crate::hub_relay::start(
        &env,
        &home,
        hub_slot.clone(),
        hub_logs.clone(),
        Some(local_dispatch.clone()),
    )
    .await;

    // One session, not a loop. The loop existed to re-authenticate after a
    // logout by returning to the login screen; the embedded core has no account
    // to log out of, so a relogin request has nowhere to go and is reported as a
    // quit rather than silently restarting an identical session. The Account
    // page's logout returns when auth itself migrates into the core.
    let status = startup_status.or(tinyplace_status).or(log_note);
    let result = run(
        &mut terminal,
        runtime.clone(),
        SessionWiring {
            loaded: loaded.clone(),
            startup_status: status,
            tinyplace_obs: tinyplace_obs.clone(),
            config_path: active_config_path.clone(),
            medulla_home: home.clone(),
            memory_service: memory_service.clone(),
            sharing,
            onboarding_path: active_config_path.clone(),
            host: local_host.as_ref().map(|host| host.observation()),
        },
    )
    .await;

    runtime.shutdown().await.ok();
    let result = result.map(|_| ());

    // Explicit teardown (the guard also runs on drop / panic).
    drop(guard);
    drop(tinyplace_service); // aborts the background loops.
    result
}
