//! The Net box: mirrored download and upload graphs beside a stats sub-box.

use super::chrome::{self, Bar};
use super::types::ResourceHits;
use crate::ui::plot::Graph;
use crate::ui::resources::sampler::{NetworkSnapshot, ResourceSnapshot};
use crate::ui::theme::{Gradient, ResourcesTheme};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    Frame,
};

/// Everything the Net box draws from besides the snapshot.
pub(super) struct NetView<'a> {
    pub config: &'a medulla::config::ResourcesConfig,
    pub theme: &'a ResourcesTheme,
    pub selected: Option<&'a str>,
    pub zero_offsets: (u64, u64),
    pub download_max: f64,
    pub upload_max: f64,
    pub auto: bool,
    pub sync: bool,
}

/// One direction's readings, already swapped when the config asks.
struct Direction {
    arrow: &'static str,
    name: &'static str,
    rate: f64,
    top: f64,
    total: u64,
    history: Vec<f32>,
    gradient: Gradient,
}

/// Stats sub-box width, as btop's.
const STATS_WIDTH: u16 = 27;

pub(super) fn draw(
    frame: &mut Frame,
    rect: Rect,
    snapshot: &ResourceSnapshot,
    view: &NetView,
    hits: &mut ResourceHits,
) {
    let theme = view.theme;
    let selected = selected_interface(snapshot, view.selected);
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, rect, theme.net_box);
    let mut top = Bar::top(rect, theme.net_box);
    hits.net_toggle = top.left(buffer, &chrome::numbered(theme, 3, "net"));
    let interface = selected.map_or("no iface", |entry| entry.interface.as_str());
    let iface = top.right(buffer, &chrome::stepper(theme, "←b", interface, "n→"));
    (hits.net_prev, hits.net_next) = chrome::stepper_hits(iface, 2, 2);
    hits.net_zero = top.right(buffer, &chrome::keyed(theme, "zero", 'z', true));
    hits.net_auto = top.right(buffer, &chrome::keyed(theme, "auto", 'a', view.auto));
    hits.net_sync = top.right(buffer, &chrome::keyed(theme, "sync", 'y', view.sync));
    let address = selected.map_or_else(|| "no IP".to_owned(), display_address);
    top.left(buffer, &chrome::plain(theme, &address));
    if inner.is_empty() {
        return;
    }
    let Some(selected) = selected else {
        chrome::text(
            frame.buffer_mut(),
            inner.x + 1,
            inner.y,
            inner.width,
            "No active network interface",
            Style::default().fg(theme.inactive_fg),
        );
        return;
    };

    let (download, upload) = directions(snapshot, selected, view);
    let boxed = inner.width >= 50 && inner.height >= 4;
    let graph_area = if boxed {
        let stats = Rect::new(
            inner.right() - STATS_WIDTH,
            inner.y,
            STATS_WIDTH,
            inner.height.min(9),
        );
        let stats = Rect::new(
            stats.x,
            inner.y + (inner.height - stats.height) / 2,
            stats.width,
            stats.height,
        );
        draw_stats(frame, stats, &download, &upload, theme);
        Rect::new(
            inner.x,
            inner.y,
            inner.width - STATS_WIDTH - 1,
            inner.height,
        )
    } else {
        inner
    };

    let (download_max, upload_max) =
        crate::ui::resources::net::synced_max(view.download_max, view.upload_max, view.sync);
    let half = graph_area.height.div_ceil(2);
    let upper = Rect::new(graph_area.x, graph_area.y, graph_area.width, half);
    let lower = Rect::new(
        graph_area.x,
        graph_area.y + half,
        graph_area.width,
        graph_area.height - half,
    );
    frame.render_widget(
        Graph::new(&download.history)
            .no_zero(true)
            .gradient(&chrome::plot_gradient(download.gradient)),
        upper,
    );
    if !lower.is_empty() {
        frame.render_widget(
            Graph::new(&upload.history)
                .invert(true)
                .no_zero(true)
                .gradient(&chrome::plot_gradient(upload.gradient)),
            lower,
        );
    }

    let buffer = frame.buffer_mut();
    let scale = Style::default().fg(theme.graph_text);
    chrome::text(
        buffer,
        graph_area.x,
        upper.y,
        8,
        &chrome::bytes_short(download_max),
        scale,
    );
    if !lower.is_empty() && lower.height > 1 {
        chrome::text(
            buffer,
            graph_area.x,
            lower.bottom() - 1,
            8,
            &chrome::bytes_short(upload_max),
            scale,
        );
    }
    if !boxed {
        // No room for the sub-box: put one line per direction over the graphs.
        for (direction, y) in [(&download, upper.y), (&upload, graph_area.bottom() - 1)] {
            let line = format!(
                "{} {}/s  Σ {}",
                direction.arrow,
                chrome::bytes(direction.rate),
                chrome::bytes(direction.total as f64)
            );
            chrome::text_right(
                buffer,
                graph_area.right(),
                y,
                graph_area.x + 6,
                &line,
                Style::default().fg(chrome::shade(&direction.gradient, 100.0)),
            );
        }
    }
}

/// The address to show for an interface: its first IPv4 address, else its
/// first address, without the prefix length.
fn display_address(interface: &NetworkSnapshot) -> String {
    let bare = |address: &String| address.split('/').next().unwrap_or_default().to_owned();
    interface
        .ip_addresses
        .iter()
        .find(|address| address.contains('.'))
        .or_else(|| interface.ip_addresses.first())
        .map_or_else(|| "no IP".to_owned(), bare)
}

fn selected_interface<'a>(
    snapshot: &'a ResourceSnapshot,
    selected: Option<&str>,
) -> Option<&'a NetworkSnapshot> {
    let find = |name: &str| {
        snapshot
            .networks
            .iter()
            .find(|entry| entry.interface == name)
    };
    selected
        .and_then(find)
        .or_else(|| crate::ui::resources::net::default_interface(&snapshot.networks).and_then(find))
}

fn directions(
    snapshot: &ResourceSnapshot,
    selected: &NetworkSnapshot,
    view: &NetView,
) -> (Direction, Direction) {
    let swap = view.config.swap_upload_download;
    let (zero_received, zero_transmitted) = view.zero_offsets;
    let history = |received: bool, max: f64| -> Vec<f32> {
        snapshot
            .history
            .network_io
            .iter()
            .filter_map(|sample| {
                sample
                    .iter()
                    .find(|entry| entry.interface == selected.interface)
            })
            .map(|entry| {
                let rate = if received {
                    entry.received_bytes_per_second
                } else {
                    entry.transmitted_bytes_per_second
                };
                (rate * 100.0 / max.max(1.0)) as f32
            })
            .collect()
    };
    let (download_max, upload_max) =
        crate::ui::resources::net::synced_max(view.download_max, view.upload_max, view.sync);
    let (top_received, top_transmitted) = network_top(snapshot, &selected.interface);
    let received = |name, arrow, max, gradient| Direction {
        arrow,
        name,
        rate: selected.received_bytes_per_second,
        top: top_received,
        total: selected.total_received_bytes.saturating_sub(zero_received),
        history: history(true, max),
        gradient,
    };
    let transmitted = |name, arrow, max, gradient| Direction {
        arrow,
        name,
        rate: selected.transmitted_bytes_per_second,
        top: top_transmitted,
        total: selected
            .total_transmitted_bytes
            .saturating_sub(zero_transmitted),
        history: history(false, max),
        gradient,
    };
    let theme = view.theme;
    if swap {
        (
            transmitted("download", "▼", download_max, theme.download),
            received("upload", "▲", upload_max, theme.upload),
        )
    } else {
        (
            received("download", "▼", download_max, theme.download),
            transmitted("upload", "▲", upload_max, theme.upload),
        )
    }
}

/// The highest rate in the retained history, each way.
pub(super) fn network_top(snapshot: &ResourceSnapshot, interface: &str) -> (f64, f64) {
    snapshot
        .history
        .network_io
        .iter()
        .flat_map(|sample| sample.iter())
        .filter(|entry| entry.interface == interface)
        .fold((0.0_f64, 0.0_f64), |(rx, tx), entry| {
            (
                rx.max(entry.received_bytes_per_second),
                tx.max(entry.transmitted_bytes_per_second),
            )
        })
}

/// The download/upload stats sub-box: rate, peak and total each way, with as
/// many of those rows as its height allows.
fn draw_stats(
    frame: &mut Frame,
    area: Rect,
    download: &Direction,
    upload: &Direction,
    theme: &ResourcesTheme,
) {
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, area, theme.net_box);
    Bar::top(area, theme.net_box).left(buffer, &chrome::plain(theme, download.name));
    Bar::bottom(area, theme.net_box).left(buffer, &chrome::plain(theme, upload.name));
    if inner.is_empty() {
        return;
    }
    let rows = if inner.height >= 7 {
        3
    } else if inner.height >= 4 {
        2
    } else {
        1
    };
    let gap = u16::from(inner.height > rows * 2);
    let main = Style::default().fg(theme.main_fg);
    for (index, direction) in [download, upload].into_iter().enumerate() {
        let arrow = Style::default()
            .fg(chrome::shade(&direction.gradient, 100.0))
            .add_modifier(Modifier::BOLD);
        let first = inner.y + index as u16 * (rows + gap);
        let lines = [
            (
                format!("{}/s", chrome::bytes(direction.rate)),
                format!("({})", chrome::bits(direction.rate)),
            ),
            (
                "Top:".to_owned(),
                format!("({})", chrome::bits(direction.top)),
            ),
            ("Total:".to_owned(), chrome::bytes(direction.total as f64)),
        ];
        let lines: Vec<_> = if rows == 2 {
            vec![lines[0].clone(), lines[2].clone()]
        } else {
            lines.into_iter().take(rows as usize).collect()
        };
        for (offset, (left, right)) in lines.iter().enumerate() {
            let y = first + offset as u16;
            if y >= inner.bottom() {
                break;
            }
            let x = chrome::text(buffer, inner.x, y, 2, direction.arrow, arrow) + 1;
            let x = chrome::text(buffer, x, y, inner.right().saturating_sub(x), left, main);
            chrome::text_right(buffer, inner.right(), y, x + 1, right, main);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_address_prefers_ipv4_and_drops_the_prefix() {
        let interface = NetworkSnapshot {
            interface: "en0".into(),
            up: true,
            connected: true,
            ip_addresses: vec![
                "fe80::1c2b:3cff:fe4d:5e6f/64".into(),
                "192.168.8.66/24".into(),
            ],
            received_bytes_per_second: 0.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 0,
            total_transmitted_bytes: 0,
        };
        assert_eq!(display_address(&interface), "192.168.8.66");
        let v6_only = NetworkSnapshot {
            ip_addresses: vec!["2001:db8::1/64".into()],
            ..interface.clone()
        };
        assert_eq!(display_address(&v6_only), "2001:db8::1");
        let none = NetworkSnapshot {
            ip_addresses: Vec::new(),
            ..interface
        };
        assert_eq!(display_address(&none), "no IP");
    }
}
