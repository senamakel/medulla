//! Data types for the `hub_relay` module.
#[allow(unused_imports)]
use super::*;
/// The shared slot the app reads for its live worker roster; filled once the
/// hub is up.
///
/// Re-exported from the SDK rather than declared again here: the app and the
/// relay have to name the *same* type for the handle to reach the Hosts tab,
/// and two structurally-identical aliases in two crates is exactly how it ended
/// up unattached.
pub(crate) use medulla::hub::HubSlot;

/// The device-local half of a hub's dispatch: the in-process bus, the address
/// the hub binds on it, and the host (if any) already listening there.
///
/// Grouped into one value because the three are meaningless apart — a bus with
/// no address bound is unreachable, and a host entry naming an address on a bus
/// the hub does not share would be a roster row nothing can deliver to.
#[derive(Clone)]
pub(crate) struct LocalDispatch {
    /// The bus shared by the hub and everything hosted in this process.
    pub(crate) network: medulla::bridge::LocalBridgeNetwork,
    /// The address the hub itself binds on that bus.
    pub(crate) hub_address: String,
    /// Every host this device declares — its bus address and what to call it —
    /// whether or not one is running.
    ///
    /// Known even when hosting is off, because it comes from `[host]` and the
    /// `[[hosts]]` entries rather than from a started host — and it is needed in
    /// exactly that case, to recognise a remembered local entry and drop it.
    ///
    /// Shared and appended to rather than a launch-time snapshot: the roster
    /// sink filters against it at *save* time, so a host added mid-session was
    /// absent from a captured list and got written into the remembered roster —
    /// a device-local entry that survives to a run where nothing binds it.
    ///
    /// One list, two readers: the roster sink and the hub, which uses it to
    /// tell its own hosts from remote peers.
    pub(crate) local_hosts: medulla::hub::SharedLocalHosts,
    /// The hosts running on this device, as roster entries. Empty when hosting
    /// is switched off — the bus is still shared, so hosts can appear later.
    pub(crate) hosts: Vec<WorkerSpec>,
}
