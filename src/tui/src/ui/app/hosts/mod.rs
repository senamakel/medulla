//! The Hosts tab and the current-host selection.
//!
//! Two things live here. The *page* — the list of `[[remoteHosts]]` machines,
//! the form that adds one, the pairing key it shows — and the *selection* the
//! `H` key drives, which the Sessions and Workflows tabs read to decide which
//! machine they are about. They share a module because the page is where the
//! selection's choices come from.
//!
//! - [`form`] — the add/edit prompt chain and what it writes.
//! - [`keys`] — the page's keyboard.
//! - [`persist`] — writing `[[remoteHosts]]` back to the user-global config.
//! - [`switcher`] — the `H` overlay and [`HostSelection`] transitions.

mod form;
mod keys;
mod persist;
mod switcher;
mod types;

#[cfg(test)]
mod tests;

pub(in crate::ui::app) use keys::HostsKey;
pub use types::{
    HostEntry, HostForm, HostFormField, HostLiveness, HostSelection, HostSwitcher, HostsState,
    PendingKey,
};

use super::remote_hosts::RemoteHostStatus;
use super::types::{App, Cmd};

/// Where the port picker starts. Above mosh's own 60000–61000: a machine that
/// already runs mosh — likely, for anyone reaching for this — has ports taken
/// there, and a pairing that collides with one fails only when the daemon
/// starts, on the other machine.
pub const FIRST_UDP_PORT: u16 = 62001;

impl App {
    /// Which machine Sessions and Workflows are scoped to.
    pub fn current_host(&self) -> &HostSelection {
        &self.current_host
    }

    /// Rebuild the page's rows from config plus the connection cache.
    ///
    /// Cheap — config times a map lookup — so it runs on every change that
    /// could move a row rather than on a schedule, and never touches the disk.
    pub(in crate::ui::app) fn rebuild_host_entries(&mut self) {
        let entries: Vec<HostEntry> =
            medulla::config::remote_hosts(&self.loaded.config.remote_hosts)
                .into_iter()
                .filter_map(|host| {
                    let section = medulla::config::remote_host_section(
                        &self.loaded.config.remote_hosts,
                        &host.id,
                    )?
                    .clone();
                    let (liveness, sessions) = self.host_liveness_of(&host.id, &section);
                    Some(HostEntry {
                        id: host.id,
                        name: host.name,
                        section,
                        liveness,
                        sessions,
                    })
                })
                .collect();
        self.hosts.index = self.hosts.index.min(entries.len().saturating_sub(1));
        self.hosts.entries = entries;
    }

    /// How `host_id` is doing, and how many sessions it reported.
    pub(in crate::ui::app) fn host_liveness_of(
        &self,
        host_id: &str,
        section: &medulla::config::RemoteHostSection,
    ) -> (HostLiveness, usize) {
        if !section.paired() {
            return (HostLiveness::Unpaired, 0);
        }
        let Some(state) = self.remote_hosts.get(host_id) else {
            return (HostLiveness::Idle, 0);
        };
        let liveness = match &state.status {
            RemoteHostStatus::Idle => HostLiveness::Idle,
            RemoteHostStatus::Connecting => HostLiveness::Connecting,
            RemoteHostStatus::Live => HostLiveness::Live,
            RemoteHostStatus::Degraded => HostLiveness::Degraded,
            RemoteHostStatus::Offline => HostLiveness::Offline,
            RemoteHostStatus::Failed(reason) => HostLiveness::Failed(reason.clone()),
        };
        (liveness, state.rows.len())
    }

    /// The row under the cursor.
    pub(in crate::ui::app) fn selected_host(&self) -> Option<&HostEntry> {
        self.hosts.entries.get(self.hosts.index)
    }

    /// What to do on entering the tab: rebuild the rows.
    pub(in crate::ui::app) fn hosts_tab_enter(&mut self) -> Option<Cmd> {
        self.rebuild_host_entries();
        None
    }

    /// Dial `host_id` unless it is already connected or connecting.
    ///
    /// The one place a connection is started from the Hosts side, so the page,
    /// the switcher and the picker all dial the same way.
    pub(in crate::ui::app) fn connect_host(&mut self, host_id: &str) -> Option<Cmd> {
        let section =
            medulla::config::remote_host_section(&self.loaded.config.remote_hosts, host_id)?;
        if !section.paired() {
            self.set_status(format!(
                "{host_id} is not paired yet — press i to issue its key"
            ));
            return None;
        }
        let should_dial = self
            .remote_hosts
            .get(host_id)
            .is_none_or(|state| state.should_dial());
        if !should_dial {
            return None;
        }
        self.remote_host_connecting(host_id);
        self.set_status(format!("Connecting to {host_id}…"));
        Some(Cmd::ConnectRemoteHost {
            host_id: host_id.to_string(),
        })
    }

    /// Drop a host's connection, if it has one.
    ///
    /// The task ends when its request channel closes, which releases the
    /// identity lock a re-pair needs.
    pub(in crate::ui::app) fn disconnect_host(&mut self, host_id: &str) {
        self.remote_hosts.remove(host_id);
    }

    /// Mint a key for `host_id` and show the command to run.
    ///
    /// A connected host is disconnected first: the identity file is rewritten
    /// underneath the dial otherwise, and the link's lock refuses that. The
    /// operator re-connects once the host has been re-paired.
    pub(in crate::ui::app) fn issue_host_key(&mut self, host_id: &str) {
        let Some(section) =
            medulla::config::remote_host_section(&self.loaded.config.remote_hosts, host_id)
                .cloned()
        else {
            self.set_status(format!("no host called {host_id}"));
            return;
        };
        let port = match section.udp_port {
            0 => self.next_free_udp_port(),
            port => port,
        };
        self.disconnect_host(host_id);
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let home = self
            .medulla_home
            .clone()
            .unwrap_or_else(|| medulla::home::medulla_home(&env));
        let state_dir = crate::remote::client::task::client_state_dir(&home, host_id);
        eprintln!("DEBUG issue_host_key host_id={} port={}", host_id, port);
        match crate::remote::client::pairing::mint_pairing(&state_dir, port) {
            Ok(key) => {
                eprintln!("DEBUG minted key.host={:?} key.pair_key={:?}", key.host, key.pair_key);
                let command = crate::remote::client::pairing::pairing_command(&key);
                if let Some(entry) = self.loaded.config.remote_hosts.iter_mut().find(|entry| {
                    medulla::config::remote_host_section(std::slice::from_ref(entry), host_id)
                        .is_some()
                }) {
                    entry.udp_port = port;
                }
                if let Err(error) = self.persist_remote_hosts() {
                    self.set_status(format!("key issued, but not saved: {error}"));
                }
                self.hosts.pending_key = Some(PendingKey {
                    host_id: host_id.to_string(),
                    command: command.clone(),
                });
                self.rebuild_host_entries();
                self.copy_line("pairing command", &command);
                let name = self
                    .selected_host()
                    .filter(|entry| entry.id == host_id)
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| host_id.to_string());
                self.set_status(format!("Run this once on {name} (copied): {command}"));
            }
            Err(error) => self.set_status(format!("could not issue a key: {error}")),
        }
    }

    /// The lowest port from [`FIRST_UDP_PORT`] no configured host already uses.
    fn next_free_udp_port(&self) -> u16 {
        let taken: Vec<u16> = self
            .loaded
            .config
            .remote_hosts
            .iter()
            .map(|host| host.udp_port)
            .collect();
        (FIRST_UDP_PORT..u16::MAX)
            .find(|port| !taken.contains(port))
            .unwrap_or(FIRST_UDP_PORT)
    }
}
