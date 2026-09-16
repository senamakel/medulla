//! In-process hub wiring for the TUI.
//!
//! The TUI spawns the hub in-process (on by default) so the workflow runs and
//! MCP dispatches this `medulla` issues reach the hosts on this device — no
//! separate process. The hub is a tokio task aborted when its guard drops (TUI
//! exit / panic). Its roster starts from this device's declared hosts plus
//! whatever was remembered from the last run, and workers are added live from
//! the Hosts tab (or pre-seeded via `MEDULLA_HUB_WORKERS`); `MEDULLA_HUB=0`
//! opts out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use medulla::hub::{start_hub, HubConfig, HubSession, WorkerSpec};

/// Default inbox poll interval when `MEDULLA_HUB_POLL_MS` is unset.
const DEFAULT_POLL_MS: u64 = 1500;

/// The config file the roster is remembered in.
///
/// Home-derived to match the rest of this module (identity dir, credentials).
/// `--config` is deliberately not consulted: the hub already resolves entirely
/// from env + home, and honouring it here alone would mean the roster and the
/// identity could come from different places.
fn roster_path(home: &Path) -> PathBuf {
    home.join("config.toml")
}

/// Workers remembered from a previous run.
///
/// Read straight from the file rather than a `LoadedConfig`, because this module
/// is reached before (and independently of) the TUI's config load, and because
/// the file it writes is the file it must read back.
fn workers_from_config(home: &Path) -> Vec<WorkerSpec> {
    let Ok(text) = std::fs::read_to_string(roster_path(home)) else {
        return Vec::new();
    };
    let Ok(config) = toml::from_str::<medulla::config::TuiConfig>(&text) else {
        return Vec::new();
    };
    config
        .hub
        .workers
        .into_iter()
        .map(|w| WorkerSpec {
            id: w.id,
            // A remembered row names a remote peer; which machine it is on is
            // the peer's own business, and this hub has never claimed to know.
            host_id: String::new(),
            address: w.address,
            name: w.label.unwrap_or_else(|| "medulla-worker".to_string()),
            description: format!("{} daemon", w.harness),
            harness: w.harness,
            // Remembered rosters carry no workspace: the local host's is
            // injected fresh each launch (see `build_hub_config_with_host`),
            // and a remote peer's is unknown here.
            workspace: None,
            // Unstated for a remote peer: its concurrency is whatever it
            // declares on its own machine. Normalised where the roster entry is
            // built.
            max_sessions: 0,
        })
        .collect()
}

/// A sink that writes roster changes back to the config file.
///
/// Best-effort and narrated: failing to save a roster must not take the hub down
/// with it, but a silent failure would leave the operator re-adding the same
/// worker every launch with no idea why.
/// The device-local hosts are deliberately *not* saved: they are derived from
/// `[host]` on every launch, so remembering them would outlive the setting that
/// produced them. A roster that kept one would, on the next run with hosting
/// off, list a worker whose address nothing binds.
fn roster_sink(
    home: &Path,
    log: medulla::hub::HubLog,
    local_hosts: medulla::hub::SharedLocalHosts,
) -> medulla::hub::RosterSink {
    let path = roster_path(home);
    Arc::new(move |workers: &[medulla::hub::HubWorker]| {
        let local_addresses: Vec<String> = local_hosts
            .lock()
            .expect("local hosts")
            .iter()
            .map(|host| host.id.clone())
            .collect();
        let rows: Vec<medulla::config::HubWorkerConfig> = workers
            .iter()
            .filter(|w| !local_addresses.contains(&w.address))
            .map(|w| medulla::config::HubWorkerConfig {
                id: w.id.clone(),
                address: w.address.clone(),
                harness: w.harness.clone(),
                label: w.label.clone(),
                selected: w.selected,
            })
            .collect();
        if let Err(e) = medulla::config::persist_hub_workers(&path, &rows) {
            log(&format!("hub: could not save the worker roster ({e})"));
        }
    })
}

/// Parse pre-seeded worker specs from the environment:
/// `MEDULLA_HUB_WORKERS="id=addr,…"`, else a single `MEDULLA_LINK_PEER`
/// (id == address). Empty is fine — the hub starts from this device's hosts
/// and workers are added live from the Hosts tab.
fn workers_from_env(env: &HashMap<String, String>) -> Vec<WorkerSpec> {
    let provider = env
        .get("MEDULLA_WORKER_PROVIDER")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude".to_string());
    let spec = |id: &str, addr: &str| WorkerSpec {
        id: id.to_string(),
        address: addr.to_string(),
        name: "medulla-worker".to_string(),
        description: format!("{provider} daemon"),
        harness: provider.clone(),
        ..Default::default()
    };
    if let Some(list) = env
        .get("MEDULLA_HUB_WORKERS")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|pair| {
                let (id, addr) = pair.split_once('=').unwrap_or((pair, pair));
                spec(id.trim(), addr.trim())
            })
            .collect();
    }
    if let Some(peer) = env
        .get("MEDULLA_LINK_PEER")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return vec![spec(peer, peer)];
    }
    Vec::new()
}

/// Whether the hub should run. **On by default** — a plain `medulla` is enough,
/// and workers are added live from the Hosts tab (or pre-seeded via
/// `MEDULLA_HUB_WORKERS`). `MEDULLA_HUB=0`/`false` is the explicit
/// kill-switch; `MEDULLA_HUB=1`/`true` is the (redundant) explicit opt-in.
fn hub_enabled(env: &HashMap<String, String>) -> bool {
    match env
        .get("MEDULLA_HUB")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        Some(v) => medulla::home::is_truthy(v),
        None => true,
    }
}

/// Build a [`HubConfig`] from the environment, or `None` when the hub should
/// not run ([`hub_enabled`]).
#[cfg(test)]
pub(crate) fn build_hub_config_with_log(
    env: &HashMap<String, String>,
    home: &Path,
    log: medulla::hub::HubLog,
) -> Option<HubConfig> {
    // No catalog: this builder is the no-host path used by probes and tests,
    // where nothing is listed for a role.
    build_hub_config_with_host(env, home, log, None)
}

/// Like [`build_hub_config_with_log`], additionally dispatching over `local` —
/// the device-local bus a host in this same process is bound to.
///
/// `local` carries the bus and, when a host is running on it, the roster entry
/// naming that host. The entry is prepended rather than appended so the machine
/// the operator is sitting at leads the list, and it replaces any remembered
/// entry with the same address so restarting never accumulates duplicates of
/// itself.
pub(crate) fn build_hub_config_with_host(
    env: &HashMap<String, String>,
    home: &Path,
    log: medulla::hub::HubLog,
    local: Option<LocalDispatch>,
) -> Option<HubConfig> {
    if !hub_enabled(env) {
        return None;
    }
    // Environment first: an explicitly exported roster is a deliberate override
    // for this run, and should not be quietly merged with a remembered one.
    let mut workers = workers_from_env(env);
    if workers.is_empty() {
        workers = workers_from_config(home);
    }
    // The device-local host is injected fresh, never inherited. Remembered
    // entries at its address are dropped first — including when no host is
    // running now, which is the case that mattered: a roster saved while
    // hosting was on would otherwise keep listing `this-device` after
    // `MEDULLA_HOST=0`, and its tasks would be routed to an address nothing
    // binds.
    let (local_network, local_address) = match &local {
        Some(dispatch) => {
            {
                let local_hosts = dispatch.local_hosts.lock().expect("local hosts");
                workers.retain(|worker| !local_hosts.iter().any(|host| host.id == worker.address));
            }
            // Inserted in declaration order, so the primary leads and the
            // extras follow it the way they read in the config.
            //
            // De-duplicated by **id**, not by address: several declared agents
            // share one host's address, and dropping every worker at that
            // address would leave only the last one inserted — a machine would
            // list one of its agents and silently lose the rest. Remembered
            // entries at a device-local address are already gone (above), so
            // this only guards against an env-seeded id colliding with a
            // declared one.
            for host in dispatch.hosts.iter().rev() {
                workers.retain(|worker| worker.id != host.id);
                workers.insert(0, host.clone());
            }
            (Some(dispatch.network.clone()), dispatch.hub_address.clone())
        }
        None => (None, String::new()),
    };
    let poll_ms = env
        .get("MEDULLA_HUB_POLL_MS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_POLL_MS);
    // The handle, not a copy of its contents: the sink reads it at save time,
    // which is the moment that knows which hosts this device is binding.
    let local_hosts = local
        .as_ref()
        .map(|dispatch| dispatch.local_hosts.clone())
        .unwrap_or_default();
    Some(HubConfig {
        local_hosts: local_hosts.clone(),
        persist: Some(roster_sink(home, log.clone(), local_hosts)),
        log,
        workers,
        poll: Duration::from_millis(poll_ms),
        local_network,
        local_address,
    })
}

/// Start the hub, fill `slot` with its live handle, and return the running
/// session (dropping it stops the hub). Returns `None` when the hub is disabled
/// (`MEDULLA_HUB=0`) or fails to start — the TUI runs fine either way.
pub(crate) async fn start(
    env: &HashMap<String, String>,
    home: &Path,
    slot: HubSlot,
    logs: medulla_tui::log::LogBuffer,
    local: Option<LocalDispatch>,
) -> Option<HubSession> {
    // The hub must never write to the terminal here: the TUI owns the alternate
    // screen, and ratatui only repaints the cells it manages, so a stray line
    // lands on top of the UI and is never cleared. Capturing them keeps the
    // screen intact and the diagnostics readable.
    let config = build_hub_config_with_host(env, home, logs.sink(), local)?;
    match start_hub(config).await {
        Ok(session) => {
            *slot.lock().expect("hub slot") = Some(session.handle.clone());
            Some(session)
        }
        Err(e) => {
            logs.push(format!("hub: failed to start ({e})"));
            None
        }
    }
}

#[cfg(test)]
mod tests;

mod types;
pub(crate) use types::{HubSlot, LocalDispatch};
