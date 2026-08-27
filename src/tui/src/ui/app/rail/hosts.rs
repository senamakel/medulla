//! The `Host → Agent` projection the Sessions rail places its lanes onto.
//!
//! This used to back the Hosts tab, which drew the tree directly and let an
//! operator edit it. That tab is gone — its pages managed an orchestration
//! engine that no longer exists — but the projection is not a management
//! surface: it is what gives a session its lane, hence its transcript and its
//! place in the rail's order. So the three reads the rail needs live here, and
//! nothing writes.

use medulla::config::LocalHostRef;
use medulla::ui::hosts::{host_rows, HostRow};

use super::super::types::App;

impl App {
    /// The hosts this machine runs, as the rail places lanes on them.
    ///
    /// Config is the source, so a host that is declared but not running is still
    /// present. A *running* primary overrides its own identity from the live
    /// observation: `[host].workspace` is usually blank ("wherever medulla was
    /// launched") and only the running host has resolved it.
    pub(in crate::ui::app) fn local_host_refs(&self) -> Vec<LocalHostRef> {
        let mut hosts =
            medulla::config::local_hosts(&self.loaded.config.host, &self.loaded.config.hosts);
        if let (Some(observation), Some(primary)) = (self.host_obs.as_ref(), hosts.first_mut()) {
            primary.id = observation.address().to_string();
            primary.workspace = observation.workspace().to_string();
            primary.name = medulla::config::local_host_name(
                &self.loaded.config.host,
                observation.workspace(),
                true,
            );
        }
        // A host this device is not serving is not a host. It stays listed only
        // while it still holds something — declared agents, or roster entries
        // that outlived the switch — because dropping those would hide agents
        // the operator wrote down rather than tidying the rail.
        let sections = std::iter::once(&self.loaded.config.host).chain(&self.loaded.config.hosts);
        let hosting = self.host_obs.is_some();
        hosts
            .into_iter()
            .zip(sections)
            .filter(|(host, section)| {
                section.enabled
                    || (host.primary && hosting)
                    || self.declares_agent_on(&host.id)
                    || self
                        .runtime
                        .workers()
                        .iter()
                        .any(|worker| worker.address.trim() == host.id)
            })
            .map(|(host, _)| host)
            .collect()
    }

    /// Whether this machine's config declares any agent on `host_id`.
    fn declares_agent_on(&self, host_id: &str) -> bool {
        self.loaded
            .config
            .fleet
            .agent_declarations
            .iter()
            .any(|declaration| declaration.on_host(host_id))
    }

    /// The `Host → Agents` tree the rail places its folded lanes onto.
    pub(in crate::ui::app) fn host_tree(&self) -> Vec<HostRow> {
        host_rows(
            &self.runtime.workers(),
            &self.loaded.config.fleet.agent_declarations,
            &self.local_host_refs(),
        )
    }
}
