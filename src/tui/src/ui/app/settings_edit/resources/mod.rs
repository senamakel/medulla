//! Btop-style editor for the Resources configuration.
//!
//! [`rows`] lists the editable fields, [`types`] defines a row, [`values`] reads
//! and formats them, and this module wires editing and drawing into `App`.

use medulla::config::{MAX_RESOURCES_UPDATE_MS, MIN_RESOURCES_UPDATE_MS};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{layout::Rect, Frame};

use crate::ui::app::types::App;

mod rows;
mod types;
mod values;

use rows::ROWS;
use types::Kind;
use values::*;

impl App {
    pub(in crate::ui::app) fn draw_resources_settings(&mut self, f: &mut Frame, area: Rect) {
        let selected = self.resources_settings_index.min(ROWS.len() - 1);
        let cfg = &self.loaded.config.resources;
        let mut lines = Vec::new();
        let mut group = "";
        let mut selected_line = 0;
        for (index, row) in ROWS.iter().enumerate() {
            if row.group != group {
                group = row.group;
                lines.push(Line::from(Span::styled(
                    group,
                    Style::default().add_modifier(Modifier::BOLD),
                )));
            }
            let value = display(cfg, *row);
            if index == selected {
                selected_line = lines.len();
            }
            let style = if index == selected {
                self.theme.selection()
            } else {
                Style::default()
            };
            let control = match row.kind {
                Kind::Flag | Kind::OptionalFlag => "toggle".to_owned(),
                Kind::Count(min, max, step) => format!("{min}–{max}, step {step}"),
                Kind::Choice(values) => format!("{} choices", values.len()),
                Kind::OptionalText => "Auto or interface".to_owned(),
            };
            lines.push(Line::from(vec![
                Span::styled(if index == selected { "▸ " } else { "  " }, style),
                Span::styled(format!("{:<25}", row.label), style),
                Span::styled(format!("‹ {value} ›   {} · {control}", row.key), style),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(ROWS[selected].help));
        lines.push(Line::from(
            "↑↓ select · ←→ change · Enter toggle · writes [resources]",
        ));
        let visible = usize::from(area.height.saturating_sub(4)).max(1);
        let scroll = selected_line.saturating_sub(visible.saturating_sub(1));
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                .block(self.content_panel("Resources")),
            area,
        );
    }

    pub(in crate::ui::app) fn move_resources_settings(&mut self, up: bool) {
        self.resources_settings_index = if up {
            self.resources_settings_index.saturating_sub(1)
        } else {
            (self.resources_settings_index + 1).min(ROWS.len() - 1)
        };
    }

    /// Toggle the selected resource setting only when it is a flag row.
    pub(in crate::ui::app) fn toggle_resources_setting(&mut self) -> String {
        let row = ROWS[self.resources_settings_index.min(ROWS.len() - 1)];
        if matches!(row.kind, Kind::Flag | Kind::OptionalFlag) {
            self.adjust_resources_setting(0)
        } else {
            format!("Resources · {} needs ← or → to change", row.label)
        }
    }

    pub(in crate::ui::app) fn adjust_resources_setting(&mut self, delta: i32) -> String {
        let row = ROWS[self.resources_settings_index.min(ROWS.len() - 1)];
        let cfg = &mut self.loaded.config.resources;
        match row.key {
            "updateMs" => {
                cfg.update_ms = step(
                    cfg.update_ms,
                    delta,
                    MIN_RESOURCES_UPDATE_MS,
                    MAX_RESOURCES_UPDATE_MS,
                    100,
                )
            }
            "shownBoxes" => {
                cfg.shown_boxes = cycle(&cfg.shown_boxes, choices(row), delta).to_owned()
            }
            "presets" => cfg.presets = cycle(&cfg.presets, choices(row), delta).to_owned(),
            "preset" => {
                let max = cfg
                    .presets
                    .split_whitespace()
                    .count()
                    .saturating_sub(1)
                    .min(9) as u64;
                cfg.preset = step(cfg.preset as u64, delta, 0, max, 1) as usize;
            }
            "cpuBottom" => cfg.cpu_bottom = optional_flag(cfg.cpu_bottom, delta),
            "memBelowNet" => cfg.mem_below_net = optional_flag(cfg.mem_below_net, delta),
            "agentsLeft" => cfg.agents_left = optional_flag(cfg.agents_left, delta),
            "cpuGraphUpper" => {
                cfg.cpu_graph_upper = cycle(&cfg.cpu_graph_upper, choices(row), delta).into()
            }
            "cpuGraphLower" => {
                cfg.cpu_graph_lower = cycle(&cfg.cpu_graph_lower, choices(row), delta).into()
            }
            "cpuInvertLower" => cfg.cpu_invert_lower = !cfg.cpu_invert_lower,
            "cpuSingleGraph" => cfg.cpu_single_graph = !cfg.cpu_single_graph,
            "tempScale" => cfg.temp_scale = cycle(&cfg.temp_scale, choices(row), delta).into(),
            "freqMode" => cfg.freq_mode = cycle(&cfg.freq_mode, choices(row), delta).into(),
            "showBattery" => cfg.show_battery = !cfg.show_battery,
            "memGraphs" => cfg.mem_graphs = !cfg.mem_graphs,
            "showDisks" => cfg.show_disks = !cfg.show_disks,
            "showSwap" => cfg.show_swap = !cfg.show_swap,
            "swapDisk" => cfg.swap_disk = !cfg.swap_disk,
            "onlyPhysical" => cfg.only_physical = !cfg.only_physical,
            "useFstab" => cfg.use_fstab = !cfg.use_fstab,
            "disksFilter" => {
                cfg.disks_filter = cycle(&cfg.disks_filter, choices(row), delta).into()
            }
            "ioMode" => cfg.io_mode = !cfg.io_mode,
            "ioGraphCombined" => cfg.io_graph_combined = !cfg.io_graph_combined,
            "ioGraphSpeeds" => {
                cfg.io_graph_speeds = cycle(&cfg.io_graph_speeds, choices(row), delta).into()
            }
            "netAuto" => cfg.net_auto = !cfg.net_auto,
            "netSync" => cfg.net_sync = !cfg.net_sync,
            "netDownload" => cfg.net_download = step(cfg.net_download, delta, 1, 1_000_000, 100),
            "netUpload" => cfg.net_upload = step(cfg.net_upload, delta, 1, 1_000_000, 100),
            "netIface" => {
                let interfaces: Vec<_> = self
                    .resource_snapshot
                    .networks
                    .iter()
                    .map(|network| network.interface.as_str())
                    .collect();
                let current = cfg
                    .net_iface
                    .as_deref()
                    .and_then(|name| interfaces.iter().position(|item| *item == name))
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let choices = interfaces.len() + 1;
                let next = if delta < 0 {
                    (current + choices - 1) % choices
                } else {
                    (current + 1) % choices
                };
                cfg.net_iface = next
                    .checked_sub(1)
                    .and_then(|index| interfaces.get(index))
                    .map(|name| (*name).to_owned());
            }
            "swapUploadDownload" => cfg.swap_upload_download = !cfg.swap_upload_download,
            _ => unreachable!("all resource settings rows are mapped"),
        }
        let clear_auto = match row.key {
            "cpuBottom" => cfg.cpu_bottom.is_none(),
            "memBelowNet" => cfg.mem_below_net.is_none(),
            "agentsLeft" => cfg.agents_left.is_none(),
            _ => false,
        };
        let value = persist_value(cfg, row);
        if let Some(path) = self.config_path.as_deref() {
            let persisted = if clear_auto {
                medulla::config::clear_setting(path, "resources", row.key)
            } else {
                medulla::config::persist_setting(path, "resources", row.key, value)
            };
            if let Err(error) = persisted {
                return format!("Resources · changed in memory; could not save: {error}");
            }
        }
        let updated = super::super::resources::ResourcesState::from_config(cfg);
        self.resources.shown = updated.shown;
        self.resources.opts = updated.opts;
        self.resources.interval_ms = updated.interval_ms;
        self.resources.preset = updated.preset;
        self.resources.presets = updated.presets;
        self.resources.show_disks = updated.show_disks;
        self.resources.io_mode = updated.io_mode;
        self.resources.net_auto = updated.net_auto;
        self.resources.net_sync = updated.net_sync;
        self.resources.net_download = updated.net_download;
        self.resources.net_upload = updated.net_upload;
        self.resources.net_iface = updated.net_iface;
        self.resources.swap_upload_download = updated.swap_upload_download;
        format!("Resources · {} → {}", row.label, display(cfg, row))
    }
}

#[cfg(test)]
mod tests;
