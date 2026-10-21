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
        }
    }
}

impl ResourcesState {
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
            opts: LayoutOptions {
                cpu_bottom: config.cpu_bottom,
                mem_below_net: config.mem_below_net,
                agents_left: config.agents_left,
            },
            interval_ms: config.update_interval_ms(),
            presets,
            ..Self::default()
        };
        state.apply_preset();
        state
    }

    pub fn apply_preset(&mut self) {
        let Some(preset) = self.presets.get(self.preset) else {
            return;
        };
        let (mut cpu_bottom, mut mem_below_net, mut agents_left) = (false, false, false);
        for item in preset.split(',') {
            let mut parts = item.split(':');
            let (Some(name), Some(position), Some(_graph)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            match name {
                "cpu" => cpu_bottom = position == "1",
                "mem" => mem_below_net = position == "1",
                "agents" => agents_left = position == "0",
                _ => {}
            }
        }
        self.opts = LayoutOptions {
            cpu_bottom,
            mem_below_net,
            agents_left,
        };
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
