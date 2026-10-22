use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ui::app) struct ShownBoxes {
    pub cpu: bool,
    pub mem: bool,
    pub net: bool,
    pub agents: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ui::app) struct LayoutOptions {
    pub cpu_bottom: bool,
    pub mem_below_net: bool,
    pub agents_left: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ui::app) struct BoxRects {
    pub cpu: Rect,
    pub mem: Rect,
    pub net: Rect,
    pub agents: Rect,
    pub too_small: Option<u16>,
}

#[derive(Debug, Clone)]
pub(in crate::ui::app) struct ResourcesState {
    pub shown: ShownBoxes,
    pub opts: LayoutOptions,
    pub rects: BoxRects,
    pub interval_ms: u64,
    pub preset: usize,
    pub presets: Vec<String>,
    pub hits: ResourceHits,
    pub menu_open: bool,
    pub show_disks: bool,
    pub io_mode: bool,
    pub net_auto: bool,
    pub net_sync: bool,
    pub net_download: u64,
    pub net_upload: u64,
    pub net_iface: Option<String>,
    pub swap_upload_download: bool,
    pub net_zeroed: bool,
    pub net_zero_received: u64,
    pub net_zero_transmitted: u64,
    pub net_download_scaler: crate::ui::resources::net::NetScaler,
    pub net_upload_scaler: crate::ui::resources::net::NetScaler,
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::ui::app) struct ResourceHits {
    pub cpu_toggle: Rect,
    pub mem_toggle: Rect,
    pub net_toggle: Rect,
    pub agents_toggle: Rect,
    pub disks_toggle: Rect,
    pub io_toggle: Rect,
    pub net_prev: Rect,
    pub net_next: Rect,
    pub net_zero: Rect,
    pub net_auto: Rect,
    pub net_sync: Rect,
    pub menu: Rect,
    pub preset_prev: Rect,
    pub preset_next: Rect,
    pub interval_down: Rect,
    pub interval_up: Rect,
}

impl Default for ResourcesState {
    fn default() -> Self {
        Self {
            shown: ShownBoxes {
                cpu: true,
                mem: true,
                net: true,
                agents: true,
            },
            opts: LayoutOptions::default(),
            rects: BoxRects::default(),
            interval_ms: 2_000,
            preset: 0,
            presets: vec!["cpu:0:default,mem:0:default,net:0:default,agents:1:default".into()],
            hits: ResourceHits::default(),
            menu_open: false,
            show_disks: true,
            io_mode: false,
            net_auto: true,
            net_sync: false,
            net_download: 100,
            net_upload: 100,
            net_iface: None,
            swap_upload_download: false,
            net_zeroed: false,
            net_zero_received: 0,
            net_zero_transmitted: 0,
            net_download_scaler: Default::default(),
            net_upload_scaler: Default::default(),
        }
    }
}

impl ResourcesState {
    pub fn sample_network(&mut self, snapshot: &crate::ui::resources::sampler::ResourceSnapshot) {
        let selected = self.net_iface.as_deref()
            .and_then(|name| snapshot.networks.iter().find(|entry| entry.interface == name))
            .or_else(|| crate::ui::resources::net::default_interface(&snapshot.networks)
                .and_then(|name| snapshot.networks.iter().find(|entry| entry.interface == name)));
        if let Some(selected) = selected {
            self.net_download_scaler.sample(selected.received_bytes_per_second);
            self.net_upload_scaler.sample(selected.transmitted_bytes_per_second);
        }
    }

    pub fn from_config(config: &medulla::config::ResourcesConfig) -> Self {
        let shown = config
            .shown_boxes
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>();
        let parsed = config
            .presets
            .split_whitespace()
            .filter(|preset| valid_preset(preset))
            .map(str::to_owned)
            .take(10)
            .collect::<Vec<_>>();
        let presets = if parsed.is_empty() {
            Self::default().presets
        } else {
            parsed
        };
        let mut state = Self {
            shown: ShownBoxes {
                cpu: shown.contains(&"cpu"),
                mem: shown.contains(&"mem"),
                net: shown.contains(&"net"),
                agents: shown.contains(&"agents"),
            },
            opts: LayoutOptions::default(),
            interval_ms: config.update_interval_ms(),
            show_disks: config.show_disks,
            io_mode: config.io_mode,
            net_auto: config.net_auto,
            net_sync: config.net_sync,
            net_download: config.net_download.max(1),
            net_upload: config.net_upload.max(1),
            net_iface: config.net_iface.clone(),
            swap_upload_download: config.swap_upload_download,
            net_zeroed: false,
            presets,
            ..Self::default()
        };
        state.preset = config.preset.min(state.presets.len().saturating_sub(1));
        state.apply_preset();
        // `shownBoxes` is an explicit startup choice. Interactive preset
        // changes replace visibility with the boxes listed by that preset.
        state.shown = ShownBoxes {
            cpu: shown.contains(&"cpu"),
            mem: shown.contains(&"mem"),
            net: shown.contains(&"net"),
            agents: shown.contains(&"agents"),
        };
        if let Some(value) = config.cpu_bottom {
            state.opts.cpu_bottom = value;
        }
        if let Some(value) = config.mem_below_net {
            state.opts.mem_below_net = value;
        }
        if let Some(value) = config.agents_left {
            state.opts.agents_left = value;
        }
        state
    }

    pub fn apply_preset(&mut self) {
        let Some(preset) = self.presets.get(self.preset) else {
            return;
        };
        let (mut cpu_bottom, mut mem_below_net, mut agents_left) = (false, false, false);
        let mut shown = ShownBoxes::default();
        for item in preset.split(',') {
            let mut parts = item.split(':');
            let (Some(name), Some(position), Some(_graph)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            match name {
                "cpu" => {
                    shown.cpu = true;
                    cpu_bottom = position == "1";
                }
                "mem" => {
                    shown.mem = true;
                    mem_below_net = position == "1";
                }
                "net" => shown.net = true,
                "agents" => agents_left = position == "0",
                _ => {}
            }
            if name == "agents" {
                shown.agents = true;
            }
        }
        self.opts = LayoutOptions {
            cpu_bottom,
            mem_below_net,
            agents_left,
        };
        self.shown = shown;
    }
}

fn valid_preset(preset: &str) -> bool {
    let mut seen = std::collections::HashSet::new();
    let mut any = false;
    for item in preset.split(',') {
        let mut parts = item.split(':');
        let (Some(name), Some(position), Some(graph)) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        if parts.next().is_some()
            || !["cpu", "mem", "net", "agents"].contains(&name)
            || !seen.insert(name)
            || !["0", "1"].contains(&position)
            || !["default", "braille", "block", "tty"].contains(&graph)
        {
            return false;
        }
        any = true;
    }
    any
}
