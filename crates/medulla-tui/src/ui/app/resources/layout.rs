use ratatui::layout::Rect;
use super::types::{BoxRects, LayoutOptions, ShownBoxes};

/// Calculate btop-style panel rectangles within the content area.
pub(super) fn calc_sizes(area: Rect, shown: ShownBoxes, opts: &LayoutOptions) -> BoxRects {
    let mut out = BoxRects::default();
    if shown.cpu && (area.width < 60 || area.height < 8) {
        out.too_small = Some(60);
        return out;
    }
    let rest = if shown.cpu {
        let h = ((u32::from(area.height) * 32 + 99) / 100).max(8).min(u32::from(area.height)) as u16;
        let cpu_y = if opts.cpu_bottom { area.bottom() - h } else { area.y };
        out.cpu = Rect::new(area.x, cpu_y, area.width, h);
        if opts.cpu_bottom { Rect::new(area.x, area.y, area.width, area.height - h) }
        else { Rect::new(area.x, area.y + h, area.width, area.height - h) }
    } else { area };
    if shown.agents && shown.mem && shown.net {
        let left_w = (u32::from(rest.width) * 45).div_ceil(100) as u16;
        let right_w = rest.width.saturating_sub(left_w);
        let top_h = (u32::from(rest.height) * 40).div_ceil(100) as u16;
        let net_h = (u32::from(rest.height) * 28).div_ceil(100) as u16;
        if opts.agents_left {
            out.agents = Rect::new(rest.x, rest.y, right_w, rest.height);
            out.mem = Rect::new(rest.x + right_w, rest.y, left_w, if opts.mem_below_net { rest.height - net_h } else { top_h });
            out.net = if opts.mem_below_net { Rect::new(rest.x + right_w, rest.y, left_w, net_h) } else { Rect::new(rest.x + right_w, rest.y + top_h, left_w, net_h) };
        } else {
            out.mem = Rect::new(rest.x, rest.y, left_w, if opts.mem_below_net { rest.height - net_h } else { top_h });
            out.net = if opts.mem_below_net { Rect::new(rest.x, rest.y, left_w, net_h) } else { Rect::new(rest.x, rest.y + top_h, left_w, net_h) };
            out.agents = Rect::new(rest.x + left_w, rest.y, right_w, rest.height);
        }
    } else if shown.agents {
        out.agents = rest;
    } else if shown.mem && shown.net {
        let mh = (u32::from(rest.height) * 59).div_ceil(100) as u16;
        out.mem = if opts.mem_below_net { Rect::new(rest.x, rest.y + rest.height - mh, rest.width, mh) } else { Rect::new(rest.x, rest.y, rest.width, mh) };
        out.net = if opts.mem_below_net { Rect::new(rest.x, rest.y, rest.width, rest.height - mh) } else { Rect::new(rest.x, rest.y + mh, rest.width, rest.height - mh) };
    } else if shown.mem { out.mem = rest; }
    else if shown.net { out.net = rest; }
    out
}
