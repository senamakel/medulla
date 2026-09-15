//! The Hosts tab and the current-host selection.
//!
//! Two things live here. The *page* — the list of `[[remoteHosts]]` machines,
//! the form that adds one, the pairing key it shows — and the *selection* the
//! `H` key drives, which the Sessions and Workflows tabs read to decide which
//! machine they are about. They share a module because the page is where the
//! selection's choices come from.

mod types;

pub use types::{
    HostEntry, HostForm, HostFormField, HostLiveness, HostSelection, HostSwitcher, HostsState,
    PendingKey,
};
