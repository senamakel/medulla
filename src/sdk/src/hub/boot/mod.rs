//! Hub bootstrap: bind the device-local bus, build the sender-runner, and
//! expose a live [`HubHandle`].
//!
//! [`start_hub`] wires everything and returns a [`HubSession`] (holding the
//! handle plus the keep-alive runner) so an embedding host can manage the
//! roster at runtime.
//!
//! The hub is a local router: it dispatches to the hosts in this process over
//! the in-memory bus. A machine elsewhere is reached by the remote-session
//! client, not through here, so an address nothing on the bus binds reports
//! plainly that there is no transport for it rather than being accepted and
//! dropped.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::bridge::RoutingBridge;

use super::handle::HubHandle;
use super::roster::SharedRoster;
use super::runner::TaskRunner;

/// The address a hub binds on the device-local bus when the caller names none.
pub const DEFAULT_LOCAL_HUB_ADDRESS: &str = "medulla-hub";

/// Build the transport + runner and return a [`HubSession`]. Errors only on
/// fatal setup (no bus to bind, or the address already taken).
pub async fn start_hub(config: HubConfig) -> anyhow::Result<HubSession> {
    // Binding is fatal rather than best-effort — a failure means something
    // else already holds this address, and silently continuing would look
    // exactly like a host that never answers.
    let Some(network) = &config.local_network else {
        anyhow::bail!("hub: no transport — attach a device-local network");
    };
    let address = match config.local_address.trim() {
        "" => DEFAULT_LOCAL_HUB_ADDRESS,
        value => value,
    };
    let local = network
        .bind(address)
        .map_err(|e| anyhow::anyhow!("hub: could not bind local address {address} ({e})"))?;
    (config.log)(&format!("hub: hosting on the local bus as {address}"));
    let hub_address = address.to_string();
    let relay: Arc<dyn super::relay::Relay> = Arc::new(RoutingBridge::local_only(local));

    // One activity log, shared by the pump that observes frames and the
    // dispatchers that send them — the Sessions view reads what both write.
    let activity = super::ActivityLog::new();
    let runner = Arc::new(TaskRunner::start_with_log_and_activity(
        relay.clone(),
        config.poll,
        config.log.clone(),
        activity.clone(),
    ));

    // The shared roster dispatch resolves against and the handle mutates.
    let roster: SharedRoster = Arc::new(Mutex::new(
        config
            .workers
            .iter()
            .map(super::roster::worker_from_spec)
            .collect(),
    ));
    (config.log)(&format!("hub: up ({} worker(s))", config.workers.len()));

    let handle = HubHandle::new(super::handle::HandleWiring {
        roster,
        address: hub_address,
        relay,
        runner: runner.clone(),
        log: config.log.clone(),
        persist: config.persist.clone(),
        activity,
    });
    Ok(HubSession {
        handle,
        _runner: runner,
    })
}

mod types;
pub use types::HubConfig;
pub use types::HubSession;
pub use types::WorkerSpec;
