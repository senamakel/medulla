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
                    state.hits.cpu_toggle = Rect::new(rect.x + 6, rect.y, 3, 1);
                    state.hits.menu = Rect::new(rect.x + 10, rect.y, 3, 1);
                    state.hits.preset_prev = Rect::new(rect.x + 14, rect.y, 3, 1);
                    state.hits.preset_next = Rect::new(rect.x + 18, rect.y, 3, 1);
                    state.hits.interval_down = Rect::new(rect.x + 22, rect.y, 3, 1);
                    state.hits.interval_up = Rect::new(rect.x + 26, rect.y, 3, 1);
                } else {
                    let offset = if title.contains("Agents") { 9 } else { 6 };
                    let hit = Rect::new(rect.x + offset, rect.y, 3, 1);
                    match title.chars().next() {
                        Some(' ') if title.contains("Mem") => state.hits.mem_toggle = hit,
                        Some(' ') if title.contains("Net") => state.hits.net_toggle = hit,
                        _ => state.hits.agents_toggle = hit,
                    }
                }
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(title.clone())
                    .border_style(Style::default().fg(color));
                if title.starts_with(" CPU ") {
                    let inner = block.inner(rect);
                    f.render_widget(Paragraph::new(Line::default()).block(block), rect);
                    draw_cpu_panel(
                        f,
                        inner,
                        &self.resource_snapshot,
                        &self.loaded.config.resources,
                        self.theme.resources.cpu,
                    );
                } else {
                    f.render_widget(Paragraph::new(Line::default()).block(block), rect);
                }
            }
        }
        if state.menu_open {
            let menu = Rect::new(
                area.right().saturating_sub(34),
                area.y + 1,
                33.min(area.width),
                3.min(area.height.saturating_sub(1)),
            );
            f.render_widget(
                Paragraph::new("Resources · 1–4 boxes · p/P presets · +/- interval")
                    .block(Block::default().borders(Borders::ALL).title(" Menu ")),
                menu,
            );
        }
    }

    pub(in crate::ui::app) fn resources_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        if !self.resources.key(key) {
            return false;
        }
        let state = &self.resources;
        let shown = [
            (state.shown.cpu, "cpu"),
            (state.shown.mem, "mem"),
            (state.shown.net, "net"),
            (state.shown.agents, "agents"),
        ]
        .into_iter()
        .filter_map(|(shown, name)| shown.then_some(name))
        .collect::<Vec<_>>()
        .join(",");
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
                (
                    "shownBoxes",
                    toml::Value::String(config.shown_boxes.clone()),
                ),
                ("preset", toml::Value::Integer(config.preset as i64)),
                (
                    "cpuBottom",
                    toml::Value::Boolean(config.cpu_bottom.unwrap_or(false)),
                ),
                (
                    "memBelowNet",
                    toml::Value::Boolean(config.mem_below_net.unwrap_or(false)),
                ),
                (
                    "agentsLeft",
                    toml::Value::Boolean(config.agents_left.unwrap_or(false)),
                ),
            ];
            for (key, value) in settings {
                if let Err(error) = medulla::config::persist_setting(path, "resources", key, value)
                {
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
            (hits.cpu_toggle, '1'),
            (hits.mem_toggle, '2'),
            (hits.net_toggle, '3'),
            (hits.agents_toggle, '4'),
            (hits.preset_prev, 'p'),
            (hits.preset_next, 'P'),
            (hits.interval_down, '-'),
            (hits.interval_up, '+'),
        ]
        .into_iter()
        .find_map(|(rect, key)| rect.contains((x, y).into()).then_some(key));
        key.is_some_and(|key| self.resources_key(crossterm::event::KeyCode::Char(key)))
    }
}

fn draw_cpu_panel(
    frame: &mut Frame,
    area: Rect,
    snapshot: &crate::ui::resources::sampler::ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
    colors: crate::ui::theme::Gradient,
) {
    if area.is_empty() {
        return;
    }
    use ratatui::layout::{Constraint, Layout};
    let history: Vec<_> = snapshot.history.cpu.iter().collect();
    let upper = medulla_monitor::CpuField::parse(&config.cpu_graph_upper)
        .unwrap_or(medulla_monitor::CpuField::Total);
    let lower = medulla_monitor::CpuField::parse(&config.cpu_graph_lower)
        .unwrap_or(medulla_monitor::CpuField::Iowait);
    let rows = if config.cpu_single_graph { 1 } else { 2 };
    let graph_rows = rows.min(area.height);
    let chunks = Layout::vertical(std::iter::repeat_n(
        Constraint::Length(1),
        graph_rows as usize,
    ))
    .split(Rect::new(area.x, area.y, area.width, graph_rows));
    let chart_gradient = crate::ui::plot::Gradient::new(vec![
        colors.start,
        colors.mid.unwrap_or(colors.start),
        colors.end.unwrap_or(colors.start),
    ]);
    let values: Vec<f32> = history
        .iter()
        .map(|sample| sample.total.field(upper))
        .collect();
    frame.render_widget(
        crate::ui::plot::Graph::new(&values).gradient(&chart_gradient),
        chunks[0],
    );
    if rows > 1 && graph_rows > 1 {
        let values: Vec<f32> = history
            .iter()
            .map(|sample| sample.total.field(lower))
            .collect();
        frame.render_widget(
            crate::ui::plot::Graph::new(&values)
                .invert(config.cpu_invert_lower)
                .gradient(&chart_gradient),
            chunks[1],
        );
    }
    let mut lines = Vec::new();
    let mut details = Vec::new();
    if !snapshot.cpu.frequencies_mhz.is_empty() {
        let average = snapshot.cpu.frequencies_mhz.iter().sum::<u64>()
            / snapshot.cpu.frequencies_mhz.len() as u64;
        details.push(format!("Freq {average} MHz"));
    }
    if let Some(temp) = snapshot.cpu.package_temperature_c {
        let temp = if config.temp_scale == "fahrenheit" {
            temp * 1.8 + 32.0
        } else {
            temp
        };
        details.push(format!(
            "Pkg {temp:.0}°{}",
            if config.temp_scale == "fahrenheit" {
                "F"
            } else {
                "C"
            }
        ));
    }
    if snapshot.cpu.load_average.iter().any(|value| *value > 0.0) {
        details.push(format!(
            "Load {:.2} {:.2} {:.2}",
            snapshot.cpu.load_average[0],
            snapshot.cpu.load_average[1],
            snapshot.cpu.load_average[2]
        ));
    }
    if let Some(uptime) = snapshot.cpu.uptime_seconds {
        details.push(format!(
            "Up {}d {:02}h",
            uptime / 86_400,
            uptime / 3_600 % 24
        ));
    }
    if config.show_battery {
        if let Some(battery) = snapshot.cpu.battery_percent {
            details.push(format!("Bat {battery:.0}%"));
        }
    }
    if !details.is_empty() {
        lines.push(ratatui::text::Line::from(details.join("  ")));
    }
    let remaining = area
        .height
        .saturating_sub(graph_rows)
        .saturating_sub(lines.len() as u16) as usize;
    let cores = &snapshot.cpu.cores;
    let per_row = if cores.len() <= remaining.max(1) {
        1
    } else {
        cores.len().div_ceil(remaining.max(1))
    };
    for group in cores
        .iter()
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(per_row.max(1))
    {
        let text = group
            .iter()
            .map(|(index, cpu)| {
                let mut row = format!("C{index} {:>3.0}%", cpu.total);
                if let Some(freq) = snapshot.cpu.frequencies_mhz.get(*index) {
                    row.push_str(&format!(" {freq}MHz"));
                }
                if let Some((_, temp, _)) = snapshot
                    .cpu
                    .core_temperatures_c
                    .iter()
                    .find(|(core, _, _)| core == index)
                {
                    let temp = if config.temp_scale == "fahrenheit" {
                        temp * 1.8 + 32.0
                    } else {
                        *temp
                    };
                    row.push_str(&format!(
                        " {temp:.0}°{}",
                        if config.temp_scale == "fahrenheit" {
                            "F"
                        } else {
                            "C"
                        }
                    ));
                }
                row
            })
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(ratatui::text::Line::from(text));
    }
    let text_area = Rect::new(
        area.x,
        area.y + graph_rows,
        area.width,
        area.height - graph_rows,
    );
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(colors.start)),
        text_area,
    );
}

fn ratatui_toml_value_u64(value: u64) -> toml::Value {
    toml::Value::Integer(value.min(i64::MAX as u64) as i64)
}

fn clock_text() -> String {
    chrono::Local::now().format("%X").to_string()
}
