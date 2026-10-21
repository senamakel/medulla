//! State, layout, keys, and rendering for the host Resources tab.

mod keys;
pub(super) mod layout;
#[cfg(test)]
mod tests;
mod types;

pub(super) use types::ResourcesState;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::types::App;

impl App {
    pub(super) fn draw_resources(&mut self, f: &mut Frame, area: Rect) {
        let state = &mut self.resources;
        let rects = layout::calc_sizes(area, state.shown, &state.opts);
        state.rects = rects;
        if let Some(required) = rects.too_small {
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
        for (shown, rect, title, color) in [
            (
                state.shown.cpu,
                rects.cpu,
                "CPU",
                self.theme.resources.cpu_box,
            ),
            (
                state.shown.mem,
                rects.mem,
                "Mem",
                self.theme.resources.mem_box,
            ),
            (
                state.shown.net,
                rects.net,
                "Net",
                self.theme.resources.net_box,
            ),
            (
                state.shown.agents,
                rects.agents,
                "Agents",
                self.theme.resources.proc_box,
            ),
        ] {
            if shown && rect.width > 0 && rect.height > 0 {
                let title = if title == "CPU" {
                    format!(
                        " CPU [1] [m] [p] [P] [-] [+] {} preset {} {}ms ",
                        clock_text(),
                        state.preset,
                        state.interval_ms
                    )
                } else {
                    let key = match title {
                        "Mem" => 2,
                        "Net" => 3,
                        _ => 4,
                    };
                    format!(" {title} [{key}] ")
                };
                if title.starts_with(" CPU ") {
                    state.hits.cpu_toggle = Rect::new(rect.x + 2 + 6, rect.y, 3, 1);
                    state.hits.menu = Rect::new(rect.x + 2 + 10, rect.y, 3, 1);
                    state.hits.preset_prev = Rect::new(rect.x + 2 + 14, rect.y, 3, 1);
                    state.hits.preset_next = Rect::new(rect.x + 2 + 18, rect.y, 3, 1);
                    state.hits.interval_down = Rect::new(rect.x + 2 + 22, rect.y, 3, 1);
                    state.hits.interval_up = Rect::new(rect.x + 2 + 26, rect.y, 3, 1);
                } else {
                    let hit = Rect::new(rect.x + 2 + 5, rect.y, 3, 1);
                    match title.chars().next() {
                        Some(' ') if title.contains("Mem") => state.hits.mem_toggle = hit,
                        Some(' ') if title.contains("Net") => state.hits.net_toggle = hit,
                        _ => state.hits.agents_toggle = hit,
                    }
                }
                f.render_widget(
                    Paragraph::new(Line::default()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(title)
                            .border_style(Style::default().fg(color)),
                    ),
                    rect,
                );
            }
        }
        if state.menu_open {
            let menu = Rect::new(area.right().saturating_sub(34), area.y + 1, 33.min(area.width), 3.min(area.height.saturating_sub(1)));
            f.render_widget(
                Paragraph::new("Resources · 1–4 boxes · p/P presets · +/- interval")
                    .block(Block::default().borders(Borders::ALL).title(" Menu ")),
                menu,
            );
        }
    }

    pub(in crate::ui::app) fn resources_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        if !self.resources.key(key) { return false; }
        let state = &self.resources;
        let shown = [
            (state.shown.cpu, "cpu"), (state.shown.mem, "mem"),
            (state.shown.net, "net"), (state.shown.agents, "agents"),
        ].into_iter().filter_map(|(shown, name)| shown.then_some(name)).collect::<Vec<_>>().join(",");
        let config = &mut self.loaded.config.resources;
        config.update_ms = state.interval_ms;
        config.shown_boxes = shown;
        config.preset = state.preset;
        config.cpu_bottom = Some(state.opts.cpu_bottom);
        config.mem_below_net = Some(state.opts.mem_below_net);
        config.agents_left = Some(state.opts.agents_left);
        if let Some(path) = self.config_path.as_deref() {
            let settings = [
                ("updateMs", ratatui_toml_value_u64(config.update_ms)),
                ("shownBoxes", toml::Value::String(config.shown_boxes.clone())),
                ("preset", toml::Value::Integer(config.preset as i64)),
                ("cpuBottom", toml::Value::Boolean(config.cpu_bottom.unwrap_or(false))),
                ("memBelowNet", toml::Value::Boolean(config.mem_below_net.unwrap_or(false))),
                ("agentsLeft", toml::Value::Boolean(config.agents_left.unwrap_or(false))),
            ];
            for (key, value) in settings {
                if let Err(error) = medulla::config::persist_setting(path, "resources", key, value) {
                    self.set_status(format!("Resources save failed: {error}"));
                    return true;
                }
            }
            self.set_status("Resources settings saved");
        } else {
            self.set_status("Resources settings changed (not persisted)");
        }
        true
    }

    pub(in crate::ui::app) fn resources_click(&mut self, x: u16, y: u16) -> bool {
        let hits = self.resources.hits;
        if hits.menu.contains((x, y).into()) {
            self.resources.menu_open = !self.resources.menu_open;
            return true;
        }
        let key = [
            (hits.cpu_toggle, '1'), (hits.mem_toggle, '2'), (hits.net_toggle, '3'),
            (hits.agents_toggle, '4'), (hits.preset_prev, 'p'), (hits.preset_next, 'P'),
            (hits.interval_down, '-'), (hits.interval_up, '+'),
        ].into_iter().find_map(|(rect, key)| rect.contains((x, y).into()).then_some(key));
        key.is_some_and(|key| self.resources_key(crossterm::event::KeyCode::Char(key)))
    }
}

fn ratatui_toml_value_u64(value: u64) -> toml::Value {
    toml::Value::Integer(value.min(i64::MAX as u64) as i64)
}

fn clock_text() -> String {
    chrono::Local::now().format("%X").to_string()
}
