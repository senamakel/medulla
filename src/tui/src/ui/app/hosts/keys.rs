//! The Hosts page's keyboard.

use crossterm::event::{KeyCode, KeyEvent};

use super::super::types::{App, Cmd};
use super::HostForm;

/// Whether the page took the key.
pub(in crate::ui::app) enum HostsKey {
    /// It did; here is what to run.
    Handled(Option<Cmd>),
    /// It did not; try the global bindings.
    Unhandled,
}

impl App {
    /// Route a key while the Hosts tab is showing.
    pub(in crate::ui::app) fn on_hosts_key(&mut self, k: KeyEvent) -> HostsKey {
        let count = self.hosts.entries.len();
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.hosts.index = self.hosts.index.saturating_sub(1);
                HostsKey::Handled(None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if count > 0 {
                    self.hosts.index = (self.hosts.index + 1).min(count - 1);
                }
                HostsKey::Handled(None)
            }
            KeyCode::Char('a') => {
                self.open_host_form(HostForm::default(), super::HostFormField::Name);
                HostsKey::Handled(None)
            }
            KeyCode::Char('e') => {
                if let Some(entry) = self.selected_host() {
                    let form = HostForm::from_section(&entry.id, &entry.section);
                    self.open_host_form(form, super::HostFormField::Name);
                } else {
                    self.set_status("No host to edit — a adds one");
                }
                HostsKey::Handled(None)
            }
            KeyCode::Char('d') => {
                match self.selected_host() {
                    Some(entry) => {
                        let (id, name) = (entry.id.clone(), entry.name.clone());
                        self.hosts.delete_armed = Some(id);
                        self.set_status(format!("Remove {name}? y confirms, anything else cancels"));
                    }
                    None => self.set_status("No host to remove"),
                }
                HostsKey::Handled(None)
            }
            KeyCode::Char('i') => {
                if let Some(id) = self.selected_host().map(|entry| entry.id.clone()) {
                    self.issue_host_key(&id);
                }
                HostsKey::Handled(None)
            }
            KeyCode::Char('y') => {
                match self.hosts.pending_key.clone() {
                    Some(key) => self.copy_line("pairing command", &key.command),
                    None => self.set_status("No key to copy — i issues one"),
                }
                HostsKey::Handled(None)
            }
            KeyCode::Char('s') => HostsKey::Handled(self.run_pairing_over_ssh()),
            KeyCode::Enter | KeyCode::Char('c') => {
                let cmd = self
                    .selected_host()
                    .map(|entry| entry.id.clone())
                    .and_then(|id| self.connect_host(&id));
                HostsKey::Handled(cmd)
            }
            KeyCode::Char('u') => {
                let cmd = self
                    .selected_host()
                    .map(|entry| entry.id.clone())
                    .and_then(|id| self.set_current_host(super::HostSelection::Host(id)));
                HostsKey::Handled(cmd)
            }
            KeyCode::Char('r') => {
                self.rebuild_host_entries();
                HostsKey::Handled(None)
            }
            KeyCode::Esc => {
                self.hosts.pending_key = None;
                HostsKey::Handled(None)
            }
            _ => HostsKey::Unhandled,
        }
    }

    /// Consume the armed removal: `y` removes, anything else cancels.
    ///
    /// Returns whether the key was consumed.
    pub(in crate::ui::app) fn consume_host_delete(&mut self, k: KeyEvent) -> bool {
        let Some(id) = self.hosts.delete_armed.take() else {
            return false;
        };
        if k.code == KeyCode::Char('y') && k.modifiers.is_empty() {
            self.remove_host(&id);
        } else {
            self.set_status("Removal cancelled");
        }
        true
    }

    /// Run the pending pairing command on the selected host over ssh.
    fn run_pairing_over_ssh(&mut self) -> Option<Cmd> {
        let entry = self.selected_host()?.clone();
        if entry.section.host.trim().is_empty() {
            self.set_status(format!("{} has no address — press e to add one", entry.name));
            return None;
        }
        let command = match &self.hosts.pending_key {
            Some(key) if key.host_id == entry.id => key.command.clone(),
            _ => {
                self.set_status("Issue a key first (i), then s runs it over ssh");
                return None;
            }
        };
        self.set_status(format!("Running the pairing command on {} over ssh…", entry.name));
        Some(Cmd::RunPairingOverSsh {
            host_id: entry.id,
            section: Box::new(entry.section),
            command,
        })
    }
}
