use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ShownBoxes {
    pub cpu: bool,
    pub mem: bool,
    pub net: bool,
    pub agents: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LayoutOptions {
    pub cpu_bottom: bool,
    pub mem_below_net: bool,
    pub agents_left: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct BoxRects {
    pub cpu: Rect,
    pub mem: Rect,
    pub net: Rect,
    pub agents: Rect,
    pub too_small: Option<u16>,
}

#[derive(Debug, Clone)]
pub(super) struct ResourcesState {
    pub shown: ShownBoxes,
    pub opts: LayoutOptions,
    pub rects: BoxRects,
    pub interval_ms: u64,
    pub preset: usize,
}

impl Default for ResourcesState {
    fn default() -> Self {
        Self {
            shown: ShownBoxes { cpu: true, mem: true, net: true, agents: true },
            opts: LayoutOptions::default(),
            rects: BoxRects::default(),
            interval_ms: 2_000,
            preset: 0,
        }
    }
}
