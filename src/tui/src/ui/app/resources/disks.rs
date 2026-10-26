//! The disks column of the Mem box, laid out as btop lays it out: each disk a
//! divider headed by its name and size, then its IO, then how full it is.

use super::chrome;
use crate::ui::plot::{Graph, Meter};
use crate::ui::resources::sampler::{DiskSnapshot, ResourceSnapshot};
use crate::ui::theme::ResourcesTheme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    Frame,
};

/// The disks to list. Swap, when it is shown as a disk, comes straight after
/// the root, where btop puts it.
pub(super) fn disk_list(
    snapshot: &ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
) -> Vec<DiskSnapshot> {
    let mut disks = snapshot.disks.clone();
    if config.swap_disk && config.show_swap && snapshot.swap_total_bytes > 0 {
        let at = disks
            .iter()
            .position(|disk| disk.mount == "/")
            .map_or(0, |root| root + 1);
        disks.insert(
            at,
            DiskSnapshot {
                device: "swap".into(),
                mount: "swap".into(),
                total_bytes: snapshot.swap_total_bytes,
                used_bytes: snapshot.swap_used_bytes,
                read_bytes_per_second: 0.0,
                write_bytes_per_second: 0.0,
                busy_percent: None,
                has_io: false,
            },
        );
    }
    disks
}

/// How each disk is drawn: a header and meter, or a single packed line when
/// even that does not fit every disk.
#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Header,
    Line,
}

pub(super) fn draw(
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
    let fit = if area.height >= count * 2 {
        Fit::Header
    } else {
        Fit::Line
    };
    // Every disk is listed before any gets an IO row; the rows left over go
    // to the disks with IO counters in list order, so the root keeps its graph
    // even when a Mac's dozen system volumes fill the column.
    let mut spare_io_rows = area.height.saturating_sub(count * 2);
    let main = Style::default().fg(theme.main_fg);
    let used_gradient = chrome::plot_gradient(theme.used);
    let mut y = area.y;
    for disk in disks {
        let io_row = fit == Fit::Header && disk.has_io && spare_io_rows > 0;
        if io_row {
            spare_io_rows -= 1;
        }
        let rows = match fit {
            Fit::Header => 2 + u16::from(io_row),
            Fit::Line => 1,
        };
        if y + rows > area.bottom() {
            break;
        }
        let percent = if disk.total_bytes == 0 {
            0.0
        } else {
            disk.used_bytes as f64 * 100.0 / disk.total_bytes as f64
        };
        let name = disk_name(&disk.mount);
        let pct_style = Style::default().fg(chrome::shade(&theme.used, percent));
        if fit == Fit::Line {
            // `root  55% ■■■■■■■■ 1.0T`
            let title = Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD);
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
        header(
            frame,
            area,
            y,
            &name,
            &chrome::bytes(disk.total_bytes as f64),
            theme,
        );
        let mut row = y + 1;
        if io_row {
            draw_disk_io(frame, area, row, snapshot, disk, config, theme);
            row += 1;
        }
        // ` Used: 55% ■■■■■■■■■■ 1013 GiB`
        let used = chrome::bytes(disk.used_bytes as f64);
        let buffer = frame.buffer_mut();
        let x = chrome::text(buffer, area.x + 1, row, 6, "Used:", main);
        let x = chrome::text(buffer, x, row, 5, &format!("{percent:>3.0}% "), pct_style);
        let meter_width = area.right().saturating_sub(x + used.len() as u16 + 2);
        chrome::text_right(buffer, area.right().saturating_sub(1), row, x, &used, main);
        frame.render_widget(
            Meter::new(percent as f32, &used_gradient, theme.meter_bg),
            Rect::new(x, row, meter_width, 1),
        );
        y += rows;
    }
}

/// `├─root──────────1.81 TiB─┤`: a divider across the column, joined to the
/// borders either side, carrying the disk's name and size.
fn header(frame: &mut Frame, area: Rect, y: u16, name: &str, size: &str, theme: &ResourcesTheme) {
    let line = Style::default().fg(theme.div_line);
    let title = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD);
    let main = Style::default().fg(theme.main_fg);
    let buffer = frame.buffer_mut();
    let left = area.x.saturating_sub(1);
    let right = area.right();
    if left < area.x {
        buffer.set_string(left, y, "├", line);
    }
    for x in area.x..right {
        buffer.set_string(x, y, "─", line);
    }
    if right < buffer.area.right() {
        buffer.set_string(right, y, "┤", line);
    }
    let name_end = chrome::text(
        buffer,
        area.x + 1,
        y,
        area.width.saturating_sub(2),
        name,
        title,
    );
    chrome::text_right(buffer, right.saturating_sub(1), y, name_end + 1, size, main);
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
