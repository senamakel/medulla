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
        let mut state = Self::default();
        state.shown = ShownBoxes {
            cpu: shown.contains(&"cpu"),
            mem: shown.contains(&"mem"),
            net: shown.contains(&"net"),
            agents: shown.contains(&"agents"),
        };
        state.opts = LayoutOptions {
            cpu_bottom: config.cpu_bottom,
            mem_below_net: config.mem_below_net,
            agents_left: config.agents_left,
        };
        state.interval_ms = config.update_interval_ms();
        state
    }
}
