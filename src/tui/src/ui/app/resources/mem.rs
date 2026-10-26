//! The Mem box: memory stats on the left and, beside a divider, the disks.

use super::chrome::{self, Bar};
use super::disks::{self, disk_list};
use super::types::ResourceHits;
use crate::ui::plot::{Graph, Meter};
use crate::ui::resources::sampler::{History, ResourceSnapshot};
use crate::ui::theme::{Gradient, ResourcesTheme};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    Frame,
};

/// One memory statistic and the gradient it is drawn in.
struct Stat<'a> {
    label: &'static str,
    value: u64,
    total: u64,
    history: &'a History<u64>,
    gradient: Gradient,
}

pub(super) fn draw(
    frame: &mut Frame,
    rect: Rect,
    snapshot: &ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
    theme: &ResourcesTheme,
    hits: &mut ResourceHits,
) {
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, rect, theme.mem_box);
    let disks = disk_list(snapshot, config);
    let split = config.show_disks && !disks.is_empty() && inner.width >= 50 && inner.height >= 3;
    let mem_width = if split { inner.width / 2 } else { inner.width };
    let mut top = Bar::top(rect, theme.mem_box);
    hits.mem_toggle = top.left(buffer, &chrome::numbered(theme, 2, "mem"));
    let mut disk_bar = if split {
        let divider = inner.x + mem_width;
        let line = Style::default().fg(theme.mem_box);
        buffer.set_string(divider, rect.y, "┬", line);
        buffer.set_string(divider, rect.bottom() - 1, "┴", line);
        for y in inner.y..inner.bottom() {
            buffer.set_string(divider, y, "│", line);
        }
        Bar::top(
            Rect::new(divider, rect.y, rect.right() - divider, 1),
            theme.mem_box,
        )
    } else {
        top
    };
    hits.disks_toggle = disk_bar.left(
        buffer,
        &chrome::keyed(theme, "disks", 'd', config.show_disks),
    );
    hits.io_toggle = disk_bar.right(buffer, &chrome::keyed(theme, "io", 'i', config.io_mode));
    if inner.is_empty() {
        return;
    }

    let mem_area = Rect::new(inner.x, inner.y, mem_width, inner.height);
    let used_rows = draw_memory(frame, mem_area, snapshot, config, theme);
    if !config.show_disks {
        return;
    }
    let disk_area = if split {
        Rect::new(
            inner.x + mem_width + 1,
            inner.y,
            inner.width - mem_width - 1,
            inner.height,
        )
    } else {
        Rect::new(
            inner.x,
            inner.y + used_rows,
            inner.width,
            inner.height.saturating_sub(used_rows),
        )
    };
    disks::draw(frame, disk_area, snapshot, &disks, config, theme);
}

/// Draw the memory column and return how many rows it used.
fn draw_memory(
    frame: &mut Frame,
    area: Rect,
    snapshot: &ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
    theme: &ResourcesTheme,
) -> u16 {
    let total = snapshot.memory_total_bytes;
    let history = &snapshot.history;
    let mut stats = vec![
        Stat {
            label: "Used",
            value: snapshot.memory_used_bytes,
            total,
            history: &history.memory_used_bytes,
            gradient: theme.used,
        },
        Stat {
            label: "Available",
            value: snapshot.memory_available_bytes,
            total,
            history: &history.memory_available_bytes,
            gradient: theme.available,
        },
        Stat {
            label: "Cached",
            value: snapshot.memory_cached_bytes,
            total,
            history: &history.memory_cached_bytes,
            gradient: theme.cached,
        },
        Stat {
            label: "Free",
            value: snapshot.memory_free_bytes,
            total,
            history: &history.memory_free_bytes,
            gradient: theme.free,
        },
    ];
    let swap_as_disk = config.swap_disk && config.show_disks;
    if config.show_swap && !swap_as_disk && snapshot.swap_total_bytes > 0 {
        stats.push(Stat {
            label: "Swap",
            value: snapshot.swap_used_bytes,
            total: snapshot.swap_total_bytes,
            history: &history.swap_used_bytes,
            gradient: theme.used,
        });
    }

    let buffer = frame.buffer_mut();
    let main = Style::default().fg(theme.main_fg);
    let value = Style::default().fg(theme.title);
    chrome::text(buffer, area.x + 1, area.y, area.width, "Total:", main);
    chrome::text_right(
        buffer,
        area.right().saturating_sub(1),
        area.y,
        area.x + 8,
        &chrome::bytes(total as f64),
        value.add_modifier(Modifier::BOLD),
    );

    // Two rows per stat when they fit (a label row, then a percentage and its
    // graph), and as many extra graph rows as the box can spare. Otherwise
    // each stat collapses onto a single row.
    let room = area.height.saturating_sub(1);
    let per = (room / stats.len().max(1) as u16).min(4);
    let mut y = area.y + 1;
    for stat in &stats {
        if y >= area.bottom() {
            break;
        }
        let percent = if stat.total == 0 {
            0.0
        } else {
            stat.value as f64 * 100.0 / stat.total as f64
        };
        let pct_style = Style::default().fg(chrome::shade(&stat.gradient, percent));
        let gradient = chrome::plot_gradient(stat.gradient);
        let samples: Vec<f32> = stat
            .history
            .iter()
            .map(|value| {
                if stat.total == 0 {
                    0.0
                } else {
                    *value as f32 * 100.0 / stat.total as f32
                }
            })
            .collect();
        if per >= 2 {
            let buffer = frame.buffer_mut();
            chrome::text(
                buffer,
                area.x + 1,
                y,
                area.width,
                &format!("{}:", stat.label),
                main,
            );
            chrome::text_right(
                buffer,
                area.right().saturating_sub(1),
                y,
                area.x + 12,
                &chrome::bytes(stat.value as f64),
                value,
            );
            let body = Rect::new(
                area.x + 6,
                y + 1,
                area.width.saturating_sub(7),
                (per - 1).min(area.bottom().saturating_sub(y + 1)),
            );
            chrome::text(
                buffer,
                area.x + 1,
                y + 1,
                5,
                &format!("{percent:>3.0}%"),
                pct_style,
            );
            if config.mem_graphs {
                frame.render_widget(Graph::new(&samples).no_zero(true).gradient(&gradient), body);
            } else {
                frame.render_widget(
                    Meter::new(percent as f32, &gradient, theme.meter_bg),
                    Rect::new(body.x, body.y, body.width, 1),
                );
            }
            y += per;
        } else {
            // `Used   37% ⣀⣀⣀⣀⣀ 17.7G`
            let buffer = frame.buffer_mut();
            let label = 10.min(area.width);
            chrome::text(buffer, area.x + 1, y, label, stat.label, main);
            let x = area.x + 1 + label;
            chrome::text(buffer, x, y, 5, &format!("{percent:>3.0}%"), pct_style);
            // Fixed width so every row's graph starts in the same column.
            let size = format!("{:>5}", chrome::bytes_short(stat.value as f64));
            let graph_x = x + 5;
            let graph_width = area.right().saturating_sub(graph_x + size.len() as u16 + 2);
            chrome::text_right(
                buffer,
                area.right().saturating_sub(1),
                y,
                graph_x,
                &size,
                value,
            );
            let graph = Rect::new(graph_x, y, graph_width, 1);
            if config.mem_graphs {
                frame.render_widget(
                    Graph::new(&samples).no_zero(true).gradient(&gradient),
                    graph,
                );
            } else {
                frame.render_widget(Meter::new(percent as f32, &gradient, theme.meter_bg), graph);
            }
            y += 1;
        }
    }
    y - area.y
}
