//! Writing the host list back to disk.
//!
//! Always the user-global file: `[[remoteHosts]]` is refused from a project
//! layer (its `sshOptions` run commands here), so a row written into one would
//! vanish on the next load. [`App::hooks_config_path`] already follows that
//! rule for the same reason, and hosts ride on it.
//!
//! [`App::hooks_config_path`]: crate::ui::app::App

use super::super::types::App;

impl App {
    /// Write `[[remoteHosts]]` as the App now holds it.
    ///
    /// # Errors
    ///
    /// When no writable config is known (the change then applies to this run
    /// only), or the file cannot be written.
    pub(in crate::ui::app) fn persist_remote_hosts(&mut self) -> Result<(), String> {
        let Some(path) = self.hooks_config_path.clone() else {
            return Err("no writable config for this run".to_string());
        };
        medulla::config::persist_remote_hosts(&path, &self.loaded.config.remote_hosts)
            .map_err(|error| error.to_string())
    }

    /// Remove `host_id` from the list, its pairing on this device, and any
    /// connection to it.
    pub(in crate::ui::app) fn remove_host(&mut self, host_id: &str) {
        self.disconnect_host(host_id);
        let sections = self.loaded.config.remote_hosts.clone();
        self.loaded.config.remote_hosts = sections
            .into_iter()
            .filter(|section| {
                medulla::config::remote_host_section(std::slice::from_ref(section), host_id)
                    .is_none()
            })
            .collect();
        if self
            .hosts
            .pending_key
            .as_ref()
            .is_some_and(|key| key.host_id == host_id)
        {
            self.hosts.pending_key = None;
        }
        if self.current_host.host_id() == Some(host_id) {
            self.current_host = super::HostSelection::ThisDevice;
        }
        // The pairing on disk is forgotten only once the config row is gone
        // for good. If the write fails the row comes back on the next launch,
        // and it must come back with the identity it dials from — otherwise a
        // full disk would turn "removed for this run" into "must re-pair".
        match self.persist_remote_hosts() {
            Ok(()) => {
                let env: std::collections::HashMap<String, String> = std::env::vars().collect();
                let home = self
                    .medulla_home
                    .clone()
                    .unwrap_or_else(|| medulla::home::medulla_home(&env));
                let state_dir = crate::remote::client::task::client_state_dir(&home, host_id);
                match crate::remote::client::pairing::forget_pairing(&state_dir) {
                    Ok(()) => self.set_status(format!("Removed {host_id}")),
                    Err(error) => self.set_status(format!(
                        "Removed {host_id}, but its pairing stays: {error}"
                    )),
                }
            }
            Err(error) => self.set_status(format!("Removed {host_id} for this run only: {error}")),
        }
        self.rebuild_host_entries();
    }
}
