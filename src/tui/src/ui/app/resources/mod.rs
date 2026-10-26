//! State, layout, keys, and rendering for the host Resources tab.

mod agent_input;
pub(in crate::ui::app) mod chrome;
mod cpu;
mod disks;
mod input;
mod keys;
pub(super) mod layout;
mod mem;
mod net;
#[cfg(test)]
mod tests;
pub(super) mod types;

pub(super) use types::ResourcesState;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use super::types::App;

impl App {
    pub(super) fn draw_resources(&mut self, f: &mut Frame, area: Rect) {
        let remote_agents: Vec<_> = self
            .remote_hosts
            .iter()
            .map(|(host, state)| (host.clone(), state.rows.clone()))
            .collect();
        chrome::fill(f.buffer_mut(), area, self.theme.resources.main_bg);
        let state = &mut self.resources;
        let rects = layout::calc_sizes(area, state.shown, &state.opts);
        state.rects = rects;
        if let Some(required) = rects.too_small {
            if state.shown.agents && area.width >= 44 && area.height >= 10 {
                state.rects.agents = area;
                crate::ui::app::render::resources::draw_agents(
                    f,
                    area,
                    &self.resource_snapshot,
                    &remote_agents,
                    state,
                    &self.theme.resources,
                );
                return;
            }
            f.render_widget(
                Paragraph::new(format!(
                    "Terminal too small for Resources (minimum {required}x8)"
                ))
                .alignment(ratatui::layout::Alignment::Center)
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                area,
            );
            return;
        }
        state.hits = types::ResourceHits::default();
        let theme = self.theme.resources;
        let config = &self.loaded.config.resources;
        let snapshot = &self.resource_snapshot;
        if state.shown.cpu && !rects.cpu.is_empty() {
            let view = cpu::CpuView {
                config,
                theme: &theme,
                preset: state.preset,
                interval_ms: state.interval_ms,
            };
            cpu::draw(f, rects.cpu, snapshot, &view, &mut state.hits);
        }
        if state.shown.mem && !rects.mem.is_empty() {
            mem::draw(f, rects.mem, snapshot, config, &theme, &mut state.hits);
        }
        if state.shown.net && !rects.net.is_empty() {
            let selected = state
                .net_iface
                .as_deref()
                .or_else(|| crate::ui::resources::net::default_interface(&snapshot.networks));
            let view = net::NetView {
                config,
                theme: &theme,
                selected,
                zero_offsets: state
                    .net_zero_offsets
                    .get(selected.unwrap_or_default())
                    .copied()
                    .unwrap_or_default(),
                download_max: if state.net_auto {
                    state.net_download_scaler.max()
                } else {
                    state.net_download as f64 * 1024.0
                },
                upload_max: if state.net_auto {
                    state.net_upload_scaler.max()
                } else {
                    state.net_upload as f64 * 1024.0
                },
                auto: state.net_auto,
                sync: state.net_sync,
            };
            net::draw(f, rects.net, snapshot, &view, &mut state.hits);
        }
        if state.shown.agents && !rects.agents.is_empty() {
            crate::ui::app::render::resources::draw_agents(
                f,
                rects.agents,
                snapshot,
                &remote_agents,
                state,
                &theme,
            );
        }
        if state.menu_open {
            overlay(
                f,
                area,
                "menu",
                &[
                    ("1-4", "show or hide a box"),
                    ("p P", "previous or next layout preset"),
                    ("- +", "sample interval"),
                    ("o", "Resources settings"),
                    ("Esc", "close"),
                ],
                &theme,
            );
        }
        if state.help_open {
            overlay(
                f,
                area,
                "help",
                &[
                    ("1-4", "show or hide boxes"),
                    ("p P", "change layout preset"),
                    ("- +", "change sample interval"),
                    ("a y", "network auto-scale and sync"),
                    ("b n", "select network interface"),
                    ("z", "reset network totals"),
                    ("d i", "disks and disk IO"),
                    ("o", "open Resources settings"),
                    ("Esc", "close"),
                ],
                &theme,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// A centred, rounded key legend drawn over the boxes.
fn overlay(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: &[(&str, &str)],
    theme: &crate::ui::theme::ResourcesTheme,
) {
    let width = entries
        .iter()
        .map(|(_, text)| text.chars().count() as u16 + 8)
        .max()
        .unwrap_or(0)
        .max(title.len() as u16 + 6)
        .min(area.width);
    let height = (entries.len() as u16 + 2).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(ratatui::widgets::Clear, rect);
    let buffer = frame.buffer_mut();
    chrome::fill(buffer, rect, theme.main_bg);
    let inner = chrome::frame(buffer, rect, theme.title);
    chrome::Bar::top(rect, theme.title).left(buffer, &chrome::plain(theme, title));
    let key = Style::default()
        .fg(theme.hi_fg)
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme.main_fg);
    for (row, (keys, description)) in entries.iter().enumerate() {
        let y = inner.y + row as u16;
        if y >= inner.bottom() {
            break;
        }
        let x = chrome::text(buffer, inner.x + 1, y, 4, keys, key);
        let x = x.max(inner.x + 6);
        chrome::text(
            buffer,
            x,
            y,
            inner.right().saturating_sub(x),
            description,
            text,
        );
    }
}

pub(super) fn io_graph_speed(config: &medulla::config::ResourcesConfig, mount: &str) -> f64 {
    config
        .io_graph_speeds
        .split_whitespace()
        .find_map(|entry| {
            let (target, speed) = entry.split_once(':')?;
            if target != mount {
                return None;
            }
            speed
                .parse::<u64>()
                .ok()
                .filter(|speed| *speed > 0)
                .map(|speed| speed as f64)
        })
        .unwrap_or(100.0)
}

pub(super) fn clock_text() -> String {
    chrono::Local::now().format("%X").to_string()
}
