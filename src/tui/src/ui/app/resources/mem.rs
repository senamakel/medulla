//! The Mem box: memory stats on the left and, beside a divider, the disks.

use super::chrome::{self, Bar};
use super::types::ResourceHits;
use crate::ui::plot::{Graph, Meter};
use crate::ui::resources::sampler::{DiskSnapshot, History, ResourceSnapshot};
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
    draw_disks(frame, disk_area, snapshot, &disks, config, theme);
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

/// The disks to list, with swap appended as a disk when configured.
fn disk_list(
    snapshot: &ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
) -> Vec<DiskSnapshot> {
    let mut disks = snapshot.disks.clone();
    if config.swap_disk && config.show_swap && snapshot.swap_total_bytes > 0 {
        disks.push(DiskSnapshot {
            device: "swap".into(),
            mount: "swap".into(),
            total_bytes: snapshot.swap_total_bytes,
            used_bytes: snapshot.swap_used_bytes,
            read_bytes_per_second: 0.0,
            write_bytes_per_second: 0.0,
            busy_percent: None,
        });
    }
    disks
}

fn draw_disks(
    frame: &mut Frame,
    area: Rect,
    snapshot: &ResourceSnapshot,
    disks: &[DiskSnapshot],
    config: &medulla::config::ResourcesConfig,
    theme: &ResourcesTheme,
) {
    if area.is_empty() || disks.is_empty() {
        return;
    }
    let count = disks.len() as u16;
    let per = if area.height >= count * 3 {
        3
    } else if area.height >= count * 2 {
        2
    } else {
        1
    };
    let main = Style::default().fg(theme.main_fg);
    let title = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD);
    let used_gradient = chrome::plot_gradient(theme.used);
    let mut y = area.y;
    for disk in disks {
        if y + per > area.bottom() && !(per == 1 && y < area.bottom()) {
            break;
        }
        let percent = if disk.total_bytes == 0 {
            0.0
        } else {
            disk.used_bytes as f64 * 100.0 / disk.total_bytes as f64
        };
        let name = disk_name(&disk.mount);
        let used = chrome::bytes(disk.used_bytes as f64);
        let pct_style = Style::default().fg(chrome::shade(&theme.used, percent));
        if per == 1 {
            // `root  55% ■■■■■■■■ 1.0T`
            let buffer = frame.buffer_mut();
            let name_width = 9.min(area.width);
            chrome::text(buffer, area.x + 1, y, name_width, &name, title);
            let x = area.x + 1 + name_width;
            chrome::text(buffer, x, y, 5, &format!("{percent:>3.0}%"), pct_style);
            let size = format!("{:>5}", chrome::bytes_short(disk.used_bytes as f64));
            let meter_x = x + 5;
            let meter_width = area.right().saturating_sub(meter_x + size.len() as u16 + 2);
            chrome::text_right(
                buffer,
                area.right().saturating_sub(1),
                y,
                meter_x,
                &size,
                main,
            );
            frame.render_widget(
                Meter::new(percent as f32, &used_gradient, theme.meter_bg),
                Rect::new(meter_x, y, meter_width, 1),
            );
            y += 1;
            continue;
        }
        let buffer = frame.buffer_mut();
        chrome::text(buffer, area.x + 1, y, area.width, &name, title);
        chrome::text_right(
            buffer,
            area.right().saturating_sub(1),
            y,
            area.x + name.chars().count() as u16 + 2,
            &chrome::bytes(disk.total_bytes as f64),
            main,
        );
        // ` Used: 55% ■■■■■■■■■■ 1013 GiB`
        let row = y + 1;
        let x = chrome::text(buffer, area.x + 1, row, 6, "Used:", main);
        let x = chrome::text(buffer, x, row, 5, &format!("{percent:>3.0}% "), pct_style);
        let meter_width = area.right().saturating_sub(x + used.len() as u16 + 2);
        chrome::text_right(buffer, area.right().saturating_sub(1), row, x, &used, main);
        frame.render_widget(
            Meter::new(percent as f32, &used_gradient, theme.meter_bg),
            Rect::new(x, row, meter_width, 1),
        );
        if per == 3 {
            draw_disk_io(frame, area, y + 2, snapshot, disk, config, theme);
        }
        y += per;
    }
}

/// One IO row under a disk: its busy percentage history, or with `io_mode`
/// its read and write rates against the configured device speed.
fn draw_disk_io(
    frame: &mut Frame,
    area: Rect,
    y: u16,
    snapshot: &ResourceSnapshot,
    disk: &DiskSnapshot,
    config: &medulla::config::ResourcesConfig,
    theme: &ResourcesTheme,
) {
    let main = Style::default().fg(theme.main_fg);
    let entries = || {
        snapshot
            .history
            .disk_io
            .iter()
            .filter_map(|sample| sample.iter().find(|entry| entry.device == disk.device))
    };
    let limit = super::io_graph_speed(config, &disk.mount) * 1024.0 * 1024.0;
    let x = area.x + 1;
    let width = area.width.saturating_sub(2);
    if config.io_mode {
        let read: Vec<f32> = entries()
            .map(|entry| (entry.read_bytes_per_second * 100.0 / limit) as f32)
            .collect();
        let write: Vec<f32> = entries()
            .map(|entry| (entry.write_bytes_per_second * 100.0 / limit) as f32)
            .collect();
        let rates = format!(
            "▲{} ▼{}",
            chrome::bytes_short(disk.read_bytes_per_second),
            chrome::bytes_short(disk.write_bytes_per_second)
        );
        let graph_width = width.saturating_sub(rates.chars().count() as u16 + 1);
        if config.io_graph_combined {
            let combined: Vec<f32> = read.iter().zip(&write).map(|(r, w)| r + w).collect();
            frame.render_widget(
                Graph::new(&combined)
                    .no_zero(true)
                    .gradient(&chrome::plot_gradient(theme.available)),
                Rect::new(x, y, graph_width, 1),
            );
        } else {
            let half = graph_width / 2;
            frame.render_widget(
                Graph::new(&read)
                    .no_zero(true)
                    .gradient(&chrome::plot_gradient(theme.free)),
                Rect::new(x, y, half, 1),
            );
            frame.render_widget(
                Graph::new(&write)
                    .no_zero(true)
                    .gradient(&chrome::plot_gradient(theme.used)),
                Rect::new(x + half, y, graph_width - half, 1),
            );
        }
        chrome::text_right(frame.buffer_mut(), area.right() - 1, y, x, &rates, main);
    } else {
        let busy: Vec<f32> = entries()
            .map(|entry| match entry.busy_percent {
                Some(busy) => busy as f32,
                None => {
                    ((entry.read_bytes_per_second + entry.write_bytes_per_second) * 100.0 / limit)
                        as f32
                }
            })
            .collect();
        let x = chrome::text(frame.buffer_mut(), x, y, 4, "IO%", main) + 1;
        frame.render_widget(
            Graph::new(&busy)
                .no_zero(true)
                .gradient(&chrome::plot_gradient(theme.available)),
            Rect::new(x, y, area.right().saturating_sub(x + 1), 1),
        );
    }
}

/// btop names the root mount `root` and the rest by their last component.
fn disk_name(mount: &str) -> String {
    if mount == "/" {
        return "root".into();
    }
    mount
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(mount)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::disk_name;

    #[test]
    fn disk_names_follow_btop() {
        assert_eq!(disk_name("/"), "root");
        assert_eq!(disk_name("/System/Volumes/Data"), "Data");
        assert_eq!(disk_name("/mnt/backup/"), "backup");
        assert_eq!(disk_name("C:\\"), "C:\\");
    }
}
