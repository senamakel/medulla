//! The `H` overlay and what changing the current host does to the rest of
//! the screen.

use crossterm::event::KeyCode;

use super::super::types::{App, Cmd, PaneView};
use super::{HostLiveness, HostSelection, HostSwitcher};

impl App {
    /// The switcher's rows: this device, everything, then each host.
    pub(in crate::ui::app) fn switcher_rows(&self) -> Vec<(HostSelection, String, HostLiveness)> {
        let mut rows = Vec::new();
        if self.local_sessions.is_some() {
            rows.push((
                HostSelection::ThisDevice,
                "This device".to_string(),
                HostLiveness::Live,
            ));
        }
        rows.push((HostSelection::All, "All hosts".to_string(), HostLiveness::Live));
        for host in medulla::config::remote_hosts(&self.loaded.config.remote_hosts) {
            let section =
                medulla::config::remote_host_section(&self.loaded.config.remote_hosts, &host.id);
            let liveness = section
                .map(|section| self.host_liveness_of(&host.id, section).0)
                .unwrap_or(HostLiveness::Unpaired);
            rows.push((HostSelection::Host(host.id), host.name, liveness));
        }
        rows
    }

    /// Open the overlay with the cursor on the current selection.
    pub(in crate::ui::app) fn open_host_switcher(&mut self) {
        let rows = self.switcher_rows();
        let index = rows
            .iter()
            .position(|(selection, _, _)| *selection == self.current_host)
            .unwrap_or(0);
        self.host_switcher = Some(HostSwitcher { rows, index });
    }

    /// A key while the overlay owns the keyboard.
    pub(in crate::ui::app) fn on_host_switcher_key(&mut self, code: KeyCode) -> Option<Cmd> {
        let Some(switcher) = self.host_switcher.as_mut() else {
            return None;
        };
        let count = switcher.rows.len();
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                switcher.index = switcher.index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('H') => {
                if count > 0 {
                    switcher.index = (switcher.index + 1) % count;
                }
            }
            KeyCode::Enter => {
                let selection = switcher.rows.get(switcher.index).map(|row| row.0.clone());
                self.host_switcher = None;
                return selection.and_then(|selection| self.set_current_host(selection));
            }
            KeyCode::Esc => self.host_switcher = None,
            _ => {}
        }
        None
    }

    /// Point Sessions and Workflows at `selection`.
    ///
    /// Resets what was cursor-relative to the old scope — the rail row, the
    /// pane, an attached harness — and dials a remote host that is not yet
    /// connected, so switching to it is also what brings it up.
    pub(in crate::ui::app) fn set_current_host(&mut self, selection: HostSelection) -> Option<Cmd> {
        let label = self
            .switcher_rows()
            .into_iter()
            .find(|(candidate, _, _)| *candidate == selection)
            .map(|(_, label, _)| label)
            .unwrap_or_else(|| "this device".to_string());
        let changed = self.current_host != selection;
        self.current_host = selection.clone();
        if changed {
            self.rail_index = 0;
            self.agent_anchor = None;
            self.pane_view = PaneView::Harness;
            self.release_harness_focus();
            #[cfg(feature = "workflows")]
            if self.tab() == "Workflows" {
                self.reload_workflows();
            }
        }
        self.set_status(format!("Host: {label}"));
        match selection {
            HostSelection::Host(id) => self.connect_host(&id),
            _ => None,
        }
    }

    /// The header's current-host label.
    pub fn current_host_label(&self) -> String {
        match &self.current_host {
            HostSelection::ThisDevice => "this device".to_string(),
            HostSelection::All => "all hosts".to_string(),
            HostSelection::Host(id) => medulla::config::remote_host_section(
                &self.loaded.config.remote_hosts,
                id,
            )
            .map(medulla::config::remote_host_name)
            .unwrap_or_else(|| id.clone()),
        }
    }
}
