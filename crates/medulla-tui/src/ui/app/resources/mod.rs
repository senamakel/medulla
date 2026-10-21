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
                    if title == "Mem" { format!(" Mem [{key}] [d] [i] ") } else { format!(" {title} [{key}] ") }
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
                        self.theme.resources.temp,
                    );
                } else {
                    f.render_widget(Paragraph::new(Line::default()).block(block), rect);
                    if title.contains("Mem") {
                        draw_mem_panel(f, block.inner(rect), &self.resource_snapshot, state.show_disks, state.io_mode);
                        state.hits.disks_toggle = Rect::new(rect.x + 9, rect.y, 3, 1);
                        state.hits.io_toggle = Rect::new(rect.x + 13, rect.y, 3, 1);
                    }
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
        config.show_disks = state.show_disks;
        config.io_mode = state.io_mode;
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
                ("showDisks", toml::Value::Boolean(config.show_disks)),
                ("ioMode", toml::Value::Boolean(config.io_mode)),
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
            (hits.disks_toggle, 'd'),
            (hits.io_toggle, 'i'),
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

fn draw_mem_panel(
    frame: &mut Frame,
    area: Rect,
    snapshot: &crate::ui::resources::sampler::ResourceSnapshot,
    show_disks: bool,
    io_mode: bool,
) {
    if area.is_empty() { return; }
    let bytes = |value: u64| {
        if value >= 1 << 30 { format!("{:.1}G", value as f64 / (1 << 30) as f64) }
        else if value >= 1 << 20 { format!("{:.0}M", value as f64 / (1 << 20) as f64) }
        else { format!("{:.0}K", value as f64 / 1024.0) }
    };
    let total = snapshot.memory_total_bytes;
    let mut lines = vec![Line::from(format!("RAM {} / {}   avail {} cached {} free {}", bytes(snapshot.memory_used_bytes), bytes(total), bytes(snapshot.memory_available_bytes), bytes(snapshot.memory_cached_bytes), bytes(snapshot.memory_free_bytes)))];
    if snapshot.swap_total_bytes > 0 {
        lines.push(Line::from(format!("Swap {} / {}", bytes(snapshot.swap_used_bytes), bytes(snapshot.swap_total_bytes))));
    }
    if show_disks {
        for disk in snapshot.disks.iter().take(area.height.saturating_sub(lines.len() as u16) as usize) {
            let used = if disk.total_bytes == 0 { 0 } else { disk.used_bytes.saturating_mul(100) / disk.total_bytes };
            let io = if io_mode { format!(" R{} W{}", bytes(disk.read_bytes_per_second as u64), bytes(disk.write_bytes_per_second as u64)) } else { disk.busy_percent.map_or(String::new(), |busy| format!(" IO{busy:.0}%")) };
            lines.push(Line::from(format!("{:<8} {:>3}% {} / {}{}", disk.mount, used, bytes(disk.used_bytes), bytes(disk.total_bytes), io)));
        }
    }
    frame.render_widget(Paragraph::new(lines).style(Style::default().fg(Color::White)), area);
}

fn draw_cpu_panel(
    frame: &mut Frame,
    area: Rect,
    snapshot: &crate::ui::resources::sampler::ResourceSnapshot,
    config: &medulla::config::ResourcesConfig,
    colors: crate::ui::theme::Gradient,
    temp_colors: crate::ui::theme::Gradient,
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
    let compact = area.width < 100;
    let graph_rows = if compact { 1 } else { rows.min(area.height) };
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
    let temp_gradient = crate::ui::plot::Gradient::new(vec![
        temp_colors.start,
        temp_colors.mid.unwrap_or(temp_colors.start),
        temp_colors.end.unwrap_or(temp_colors.start),
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
    if !compact {
        if let Some(model) = snapshot
            .cpu
            .model
            .as_deref()
            .filter(|model| !model.is_empty())
        {
            details.push(model.to_owned());
        }
    }
    let frequencies: Vec<u64> = snapshot
        .cpu
        .frequencies_mhz
        .iter()
        .flatten()
        .copied()
        .collect();
    if !frequencies.is_empty() {
        let average = frequencies.iter().sum::<u64>() / frequencies.len() as u64;
        details.push(format!(
            "Freq {}",
            format_frequency(average, &config.freq_mode)
        ));
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
    {
        details.push(format!(
            "Load avg {} {} {}",
            if compact {
                format!("{:.1}", snapshot.cpu.load_average[0])
            } else {
                format!("{:.2}", snapshot.cpu.load_average[0])
            },
            if compact {
                format!("{:.1}", snapshot.cpu.load_average[1])
            } else {
                format!("{:.2}", snapshot.cpu.load_average[1])
            },
            if compact {
                format!("{:.1}", snapshot.cpu.load_average[2])
            } else {
                format!("{:.2}", snapshot.cpu.load_average[2])
            }
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
    let rows_per_core = if compact { 15 } else { 1 };
    let max_per_row = (area.width as usize / rows_per_core).max(1);
    let per_row = if cores.len() <= remaining.max(1) {
        1
    } else {
        cores.len().div_ceil(remaining.max(1))
    }
    .min(max_per_row);
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
                if let Some(Some(freq)) = snapshot.cpu.frequencies_mhz.get(*index) {
                    row.push_str(&format!(" {}", format_frequency(*freq, &config.freq_mode)));
                }
                if !compact {
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
    if area.width >= 100 {
        let graph_x = area.x + (area.width * 2 / 3).max(20);
        let graph_width = area.right().saturating_sub(graph_x);
        let details_rows = u16::from(!details.is_empty());
        for (row_index, group) in cores
            .iter()
            .enumerate()
            .collect::<Vec<_>>()
            .chunks(per_row.max(1))
            .enumerate()
        {
            let y = area.y + graph_rows + details_rows + row_index as u16;
            if y >= area.bottom() {
                break;
            }
            let segment_width = graph_width / group.len().max(1) as u16;
            for (group_index, (core_index, _)) in group.iter().enumerate() {
                let segment_x = graph_x + segment_width * group_index as u16;
                if segment_width <= 3 {
                    continue;
                }
                let usage: Vec<f32> = history
                    .iter()
                    .filter_map(|sample| sample.cores.get(*core_index).map(|core| core.total))
                    .collect();
                let usage_width = segment_width / 2;
                frame.render_widget(
                    crate::ui::plot::Graph::new(&usage).gradient(&chart_gradient),
                    Rect::new(segment_x, y, usage_width, 1),
                );
                let temps: Vec<f32> = history
                    .iter()
                    .filter_map(|sample| {
                        sample
                            .core_temperatures_c
                            .iter()
                            .find(|(index, _, _)| index == core_index)
                            .map(|(_, temp, crit)| {
                                if *crit > 0.0 {
                                    temp * 100.0 / crit
                                } else {
                                    *temp
                                }
                            })
                    })
                    .collect();
                if !temps.is_empty() && usage_width < segment_width {
                    frame.render_widget(
                        crate::ui::plot::Graph::new(&temps).gradient(&temp_gradient),
                        Rect::new(segment_x + usage_width, y, segment_width - usage_width, 1),
                    );
                }
            }
        }
    }
}

fn format_frequency(mhz: u64, mode: &str) -> String {
    match mode {
        "mhz" => format!("{mhz}MHz"),
        "ghz" => format!("{:.1}GHz", mhz as f64 / 1000.0),
        _ if mhz >= 1000 => format!("{:.1}GHz", mhz as f64 / 1000.0),
        _ => format!("{mhz}MHz"),
    }
}

fn ratatui_toml_value_u64(value: u64) -> toml::Value {
    toml::Value::Integer(value.min(i64::MAX as u64) as i64)
}

fn clock_text() -> String {
    chrono::Local::now().format("%X").to_string()
}
