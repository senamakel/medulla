//! The Resources tab's keys and clicks, outside the Agents box.

use crate::ui::app::types::App;

impl App {
    pub(in crate::ui::app) fn resources_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        // An open filter prompt takes every key, letters that are shortcuts
        // elsewhere in the tab included.
        if self.resources.agent_filter_editing && self.resources.shown.agents {
            return self.agent_key(key);
        }
        if key == crossterm::event::KeyCode::Char('o') {
            self.tab_index = crate::ui::app::TABS
                .iter()
                .position(|tab| *tab == "Settings")
                .unwrap_or(0);
            self.settings_index = crate::ui::app::types::SP_RESOURCES;
            self.settings_focused = true;
            self.set_status("Settings · Resources");
            return true;
        }
        if key == crossterm::event::KeyCode::Char('m') {
            self.resources.menu_open = !self.resources.menu_open;
            return true;
        }
        if key == crossterm::event::KeyCode::Char('?') {
            self.resources.help_open = !self.resources.help_open;
            return true;
        }
        if self.resources.help_open {
            if key == crossterm::event::KeyCode::Esc {
                self.resources.help_open = false;
            }
            return true;
        }
        if self.agent_key(key) {
            return true;
        }
        if !self.resources.key(key) {
            return false;
        }
        if key == crossterm::event::KeyCode::Char('z') {
            let current = self.resources.net_iface.as_deref().or_else(|| {
                crate::ui::resources::net::default_interface(&self.resource_snapshot.networks)
            });
            if let Some(interface) = current.and_then(|name| {
                self.resource_snapshot
                    .networks
                    .iter()
                    .find(|entry| entry.interface == name)
            }) {
                self.resources.net_zero_offsets.insert(
                    interface.interface.clone(),
                    (
                        interface.total_received_bytes,
                        interface.total_transmitted_bytes,
                    ),
                );
                self.resources.net_iface = Some(interface.interface.clone());
            }
        }
        if matches!(key, crossterm::event::KeyCode::Char('b' | 'n')) {
            let current = self.resources.net_iface.as_deref().or_else(|| {
                crate::ui::resources::net::default_interface(&self.resource_snapshot.networks)
            });
            let interfaces: Vec<_> = self
                .resource_snapshot
                .networks
                .iter()
                .map(|interface| (interface.interface.as_str(), interface.up))
                .collect();
            self.resources.net_iface = crate::ui::resources::net::cycle_interface(
                current,
                &interfaces,
                key == crossterm::event::KeyCode::Char('n'),
            );
        }
        let state = &self.resources;
        let shown = [
            (state.shown.cpu, "cpu"),
            (state.shown.mem, "mem"),
            (state.shown.net, "net"),
            (state.shown.agents, "agents"),
        ]
        .into_iter()
        .filter_map(|(shown, name)| shown.then_some(name))
        .collect::<Vec<_>>()
        .join(",");
        let config = &mut self.loaded.config.resources;
        config.update_ms = state.interval_ms;
        config.shown_boxes = shown;
        config.preset = state.preset;
        config.cpu_bottom = Some(state.opts.cpu_bottom);
        config.mem_below_net = Some(state.opts.mem_below_net);
        config.agents_left = Some(state.opts.agents_left);
        config.show_disks = state.show_disks;
        config.io_mode = state.io_mode;
        config.net_auto = state.net_auto;
        config.net_sync = state.net_sync;
        config.net_download = state.net_download;
        config.net_upload = state.net_upload;
        config.net_iface = state.net_iface.clone();
        config.swap_upload_download = state.swap_upload_download;
        if let Some(path) = self.config_path.as_deref() {
            let settings = [
                ("updateMs", ratatui_toml_value_u64(config.update_ms)),
                (
                    "shownBoxes",
                    toml::Value::String(config.shown_boxes.clone()),
                ),
                ("preset", toml::Value::Integer(config.preset as i64)),
                (
                    "cpuBottom",
                    toml::Value::Boolean(config.cpu_bottom.unwrap_or(false)),
                ),
                (
                    "memBelowNet",
                    toml::Value::Boolean(config.mem_below_net.unwrap_or(false)),
                ),
                (
                    "agentsLeft",
                    toml::Value::Boolean(config.agents_left.unwrap_or(false)),
                ),
                ("showDisks", toml::Value::Boolean(config.show_disks)),
                ("ioMode", toml::Value::Boolean(config.io_mode)),
                ("netAuto", toml::Value::Boolean(config.net_auto)),
                ("netSync", toml::Value::Boolean(config.net_sync)),
                ("netDownload", ratatui_toml_value_u64(config.net_download)),
                ("netUpload", ratatui_toml_value_u64(config.net_upload)),
                (
                    "netIface",
                    config
                        .net_iface
                        .clone()
                        .map_or(toml::Value::String(String::new()), toml::Value::String),
                ),
                (
                    "swapUploadDownload",
                    toml::Value::Boolean(config.swap_upload_download),
                ),
            ];
            for (key, value) in settings {
                if let Err(error) = medulla::config::persist_setting(path, "resources", key, value)
                {
                    self.set_status(format!("Resources save failed: {error}"));
                    return true;
                }
            }
            self.set_status("Resources settings saved");
        } else {
            self.set_status("Resources settings changed (not persisted)");
        }
        true
    }

    pub(in crate::ui::app) fn resources_click(&mut self, x: u16, y: u16) -> bool {
        let hits = self.resources.hits;
        if hits.menu.contains((x, y).into()) {
            self.resources.menu_open = !self.resources.menu_open;
            return true;
        }
        let key = [
            (hits.cpu_toggle, '1'),
            (hits.mem_toggle, '2'),
            (hits.net_toggle, '3'),
            (hits.agents_toggle, '4'),
            (hits.disks_toggle, 'd'),
            (hits.io_toggle, 'i'),
            (hits.net_prev, 'b'),
            (hits.net_next, 'n'),
            (hits.net_zero, 'z'),
            (hits.net_auto, 'a'),
            (hits.net_sync, 'y'),
            (hits.preset_prev, 'p'),
            (hits.preset_next, 'P'),
            (hits.interval_down, '-'),
            (hits.interval_up, '+'),
        ]
        .into_iter()
        .find_map(|(rect, key)| rect.contains((x, y).into()).then_some(key));
        key.is_some_and(|key| self.resources_key(crossterm::event::KeyCode::Char(key)))
    }
}

fn ratatui_toml_value_u64(value: u64) -> toml::Value {
    toml::Value::Integer(value.min(i64::MAX as u64) as i64)
}
