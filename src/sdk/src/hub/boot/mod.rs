//! Hub bootstrap: bring up the host link, build the bridge + sender-runner,
//! and expose a live [`HubHandle`].
//!
//! [`start_hub`] wires everything and returns a [`HubSession`] (holding the
//! handle plus the keep-alive runner) so an embedding host can manage the
//! roster at runtime.
//!
//! A hub without a link identity is not an error. It becomes a local-only
//! router: it still dispatches to the host in this process, and a remote
//! address reports plainly that there is no transport for it rather than being
//! accepted and dropped.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use medulla_link::{Link, LinkConfig, PeerConfig};

use crate::bridge::{LinkBridge, LinkBridgeConfig, RoutingBridge};

use super::handle::HubHandle;
use super::roster::SharedRoster;
use super::runner::TaskRunner;

/// The address a hub binds on the device-local bus when the caller names none.
pub const DEFAULT_LOCAL_HUB_ADDRESS: &str = "medulla-hub";

/// Build the transport + runner and return a [`HubSession`]. Errors only on
/// fatal setup (unusable link identity, no transport at all).
pub async fn start_hub(config: HubConfig) -> anyhow::Result<HubSession> {
    // The remote half. `None` is the ordinary single-device case, not a failure.
    let remote = match &config.link {
        Some(link) => Some(connect_link(link, &config.log).await?),
        None => None,
    };
    let hub_address = remote
        .as_ref()
        .map(|bridge| bridge.address().to_string())
        .unwrap_or_else(|| config.local_address.clone());

    // With a local bus attached the relay becomes a router: workers hosted in
    // this process are reached in memory and never touch the link, while remote
    // ones fall through unchanged. Binding is fatal rather than best-effort —
    // a failure means something else already holds this address, and silently
    // continuing remote-only would look exactly like a host that never answers.
    let relay: Arc<dyn super::relay::Relay> = match (&config.local_network, remote) {
        (Some(network), remote) => {
            let address = match config.local_address.trim() {
                "" => DEFAULT_LOCAL_HUB_ADDRESS,
                value => value,
            };
            let local = network.bind(address).map_err(|e| {
                anyhow::anyhow!("hub: could not bind local address {address} ({e})")
            })?;
            (config.log)(&format!("hub: also hosting on the local bus as {address}"));
            match remote {
                Some(remote) => Arc::new(RoutingBridge::new(
                    local,
                    Arc::new(remote) as Arc<dyn crate::bridge::Bridge>,
                )),
                None => Arc::new(RoutingBridge::local_only(local)),
            }
        }
        (None, Some(remote)) => Arc::new(remote),
        (None, None) => anyhow::bail!(
            "hub: no transport — configure a host link or attach a device-local network"
        ),
    };

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
        local_hosts: config.local_hosts.clone(),
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

/// Bring up the link and wrap it as a bridge.
///
/// The peer table is supplied by the caller rather than discovered: node names
/// and ids are issued together at enrollment (protocol §2), so by the time a hub
/// starts there is nothing left to resolve.
async fn connect_link(
    config: &HubLinkConfig,
    log: &super::types::HubLog,
) -> anyhow::Result<LinkBridge> {
    let handle = match &config.handle {
        Some(handle) => handle.clone(),
        None => {
            let mut link_config = LinkConfig::new(config.state_dir.clone());
            link_config.path = medulla_link::LinkPath::Forwarder {
                endpoint: config.forwarder_endpoint.clone(),
            };
            link_config.peers = config
                .peers
                .iter()
                .map(|peer| PeerConfig {
                    node_id: peer.node_id,
                    pair_key: peer.pair_key.clone(),
                })
                .collect();
            Arc::new(
                Link::connect(link_config)
                    .await
                    .map_err(|e| anyhow::anyhow!("hub: could not bring up the host link ({e})"))?,
            )
        }
    };
    log(&format!(
        "hub: link up as {} ({} peer(s))",
        config.node_name,
        config.peers.len()
    ));
    Ok(LinkBridge::new(
        handle,
        LinkBridgeConfig {
            node_name: config.node_name.clone(),
            peers: config
                .peers
                .iter()
                .map(|peer| crate::bridge::LinkPeer {
                    name: peer.name.clone(),
                    node_id: peer.node_id,
                })
                .collect(),
        },
    ))
}

mod types;
pub use types::HubConfig;
pub use types::HubLinkConfig;
pub use types::HubLinkPeer;
pub use types::HubSession;
pub use types::WorkerSpec;
