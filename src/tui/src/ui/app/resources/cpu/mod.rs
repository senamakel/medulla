//! The CPU box: a full-height usage history beside a per-core sub-box.

use super::chrome::{self, Bar};
use super::types::ResourceHits;
use crate::ui::plot::Graph;
use crate::ui::resources::sampler::ResourceSnapshot;
use crate::ui::resources::CpuField;
use crate::ui::theme::ResourcesTheme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    Frame,
};

/// Everything the CPU box draws from besides the snapshot.
pub(super) struct CpuView<'a> {
    pub config: &'a medulla::config::ResourcesConfig,
    pub theme: &'a ResourcesTheme,
    pub preset: usize,
    pub interval_ms: u64,
}

/// Geometry of the per-core sub-box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoreLayout {
    columns: usize,
    rows: usize,
    label: u16,
    graph: u16,
    temp: bool,
    /// Outer width of the sub-box, borders included.
    width: u16,
}

/// Width of a core's percentage field, ` 100%`.
const PCT: u16 = 5;
/// Width of a core's temperature field, ` 45°C`.
const TEMP: u16 = 5;
/// Width of one core in the compact layout, `C10 100% `.
const COMPACT_CORE: u16 = 10;

pub(super) fn draw(
    frame: &mut Frame,
    rect: Rect,
    snapshot: &ResourceSnapshot,
    view: &CpuView,
    hits: &mut ResourceHits,
) {
    let theme = view.theme;
    let config = view.config;
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, rect, theme.cpu_box);
    let mut top = Bar::top(rect, theme.cpu_box);
    hits.cpu_toggle = top.left(buffer, &chrome::numbered(theme, 1, "cpu"));
    hits.menu = top.left(buffer, &chrome::keyed(theme, "menu", 'm', true));
    let preset = top.left(
        buffer,
        &chrome::stepper(theme, "‹", &format!("preset {}", view.preset), "›"),
    );
    (hits.preset_prev, hits.preset_next) = chrome::stepper_hits(preset, 1, 1);
    let interval = top.right(
        buffer,
        &chrome::stepper(theme, "-", &format!("{}ms", view.interval_ms), "+"),
    );
    (hits.interval_down, hits.interval_up) = chrome::stepper_hits(interval, 1, 1);
    if config.show_battery {
        if let Some(battery) = snapshot.cpu.battery_percent {
            top.right(buffer, &battery_spans(theme, battery));
        }
    }
    top.center(buffer, &chrome::plain(theme, &super::clock_text()));
    if inner.is_empty() {
        return;
    }

    let temps = !snapshot.cpu.core_temperatures_c.is_empty();
    let layout = core_layout(inner, snapshot.cpu.cores.len(), temps);
    let graph_area = match layout {
        Some(layout) => {
            // Sized to its rows and centred, as btop places it.
            let height = (layout.rows as u16 + 4).min(inner.height);
            let cores = Rect::new(
                inner.right() - layout.width,
                inner.y + (inner.height - height) / 2,
                layout.width,
                height,
            );
            draw_cores(frame, cores, snapshot, view, layout);
            Rect::new(
                inner.x,
                inner.y,
                inner.width.saturating_sub(layout.width + 1),
                inner.height,
            )
        }
        None => {
            // No room for the sub-box: the cores become text rows under the
            // graph, then a one-row summary.
            let per_line = usize::from(inner.width / COMPACT_CORE).max(1);
            let core_lines = (snapshot.cpu.cores.len().div_ceil(per_line) as u16)
                .min(inner.height.saturating_sub(1));
            let summary_y = inner.bottom() - 1;
            let cores_y = summary_y - core_lines;
            draw_compact_cores(frame, inner, cores_y, core_lines, per_line, snapshot, view);
            draw_summary(frame, inner, summary_y, snapshot, view);
            Rect::new(inner.x, inner.y, inner.width, cores_y - inner.y)
        }
    };
    draw_history(frame, graph_area, snapshot, view);
}

/// The usage history: the upper field grows upwards from the middle and the
/// lower field, unless the box is single-graph, grows downwards beneath it.
fn draw_history(frame: &mut Frame, area: Rect, snapshot: &ResourceSnapshot, view: &CpuView) {
    if area.is_empty() {
        return;
    }
    let config = view.config;
    let gradient = chrome::plot_gradient(view.theme.cpu);
    let upper = CpuField::parse(&config.cpu_graph_upper).unwrap_or(CpuField::Total);
    let lower = CpuField::parse(&config.cpu_graph_lower).unwrap_or(CpuField::Iowait);
    let series = |field: CpuField| -> Vec<f32> {
        snapshot
            .history
            .cpu
            .iter()
            .map(|sample| sample.total.field(field))
            .collect()
    };
    let lower_values = series(lower);
    // A lower field the platform never reports (iowait on macOS) would leave
    // half the box permanently empty, so the upper graph takes it over.
    let lower_reported = lower_values.iter().any(|value| *value > 0.0);
    let split = !config.cpu_single_graph && lower_reported && area.height >= 2;
    let upper_height = if split {
        area.height.div_ceil(2)
    } else {
        area.height
    };
    frame.render_widget(
        Graph::new(&series(upper)).gradient(&gradient),
        Rect::new(area.x, area.y, area.width, upper_height),
    );
    if split {
        frame.render_widget(
            Graph::new(&lower_values)
                .invert(config.cpu_invert_lower)
                .gradient(&gradient),
            Rect::new(
                area.x,
                area.y + upper_height,
                area.width,
                area.height - upper_height,
            ),
        );
    }
    if let Some(uptime) = snapshot.cpu.uptime_seconds {
        let text = format!(
            "up {}d {:02}:{:02}",
            uptime / 86_400,
            uptime / 3_600 % 24,
            uptime / 60 % 60
        );
        if area.width as usize > text.len() + 2 {
            chrome::text(
                frame.buffer_mut(),
                area.x + 1,
                area.bottom() - 1,
                text.len() as u16,
                &text,
                Style::default().fg(view.theme.main_fg),
            );
        }
    }
}

/// Fit the per-core sub-box, preferring wide core graphs and temperatures
/// and giving them up before giving up the box.
fn core_layout(inner: Rect, count: usize, temps: bool) -> Option<CoreLayout> {
    if inner.width < 60 || inner.height < 5 {
        return None;
    }
    // A snapshot taken before the first core reading still gets the box, so
    // its CPU meter and load average keep their place.
    let count = count.max(1);
    let rows_available = usize::from(inner.height - 4).max(1);
    let columns = count.div_ceil(rows_available);
    let rows = count.div_ceil(columns);
    let label = (format!("C{}", count - 1).len() as u16 + 1).max(4);
    let max_width = inner.width * 55 / 100;
    let attempts = [
        (temps, 10),
        (temps, 8),
        (temps, 6),
        (false, 6),
        (false, 4),
        (false, 0),
    ];
    attempts.into_iter().find_map(|(temp, graph)| {
        let column = label + graph + PCT + if temp { TEMP } else { 0 };
        let content = columns as u16 * column + columns.saturating_sub(1) as u16;
        let width = content.max(28) + 2;
        (width <= max_width).then_some(CoreLayout {
            columns,
            rows,
            label,
            graph,
            temp,
            width,
        })
    })
}

fn draw_cores(
    frame: &mut Frame,
    area: Rect,
    snapshot: &ResourceSnapshot,
    view: &CpuView,
    layout: CoreLayout,
) {
    let theme = view.theme;
    let config = view.config;
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, area, theme.cpu_box);
    let mut top = Bar::top(area, theme.cpu_box);
    let frequencies: Vec<u64> = snapshot
        .cpu
        .frequencies_mhz
        .iter()
        .flatten()
        .copied()
        .collect();
    if !frequencies.is_empty() {
        let average = frequencies.iter().sum::<u64>() / frequencies.len() as u64;
        top.right(
            buffer,
            &chrome::plain(theme, &format_frequency(average, &config.freq_mode)),
        );
    }
    if let Some(model) = snapshot.cpu.model.as_deref().filter(|m| !m.is_empty()) {
        let room = area.width.saturating_sub(16) as usize;
        let model = shorten_model(model, room);
        if !model.is_empty() {
            top.left(
                buffer,
                &[Span::styled(
                    model,
                    Style::default()
                        .fg(theme.title)
                        .add_modifier(Modifier::BOLD),
                )],
            );
        }
    }
    if inner.is_empty() {
        return;
    }

    let main = Style::default().fg(theme.main_fg);
    let total = f64::from(snapshot.cpu.total.total);
    let temp = snapshot.cpu.package_temperature_c.map(|temp| {
        let crit = snapshot.cpu.package_temperature_crit_c.unwrap_or(100.0);
        (temp, crit)
    });
    let temp_width = if temp.is_some() { 6 } else { 0 };
    let x = chrome::text(buffer, inner.x, inner.y, 4, "CPU ", main);
    let meter_width = inner.width.saturating_sub(4 + PCT + temp_width);
    frame.render_widget(
        crate::ui::plot::Meter::new(
            total as f32,
            &chrome::plot_gradient(theme.cpu),
            theme.meter_bg,
        ),
        Rect::new(x, inner.y, meter_width, 1),
    );
    let buffer = frame.buffer_mut();
    chrome::text(
        buffer,
        x + meter_width,
        inner.y,
        PCT,
        &format!("{:>4.0}%", total),
        Style::default().fg(chrome::shade(&theme.cpu, total)),
    );
    if let Some((temp, crit)) = temp {
        chrome::text(
            buffer,
            x + meter_width + PCT,
            inner.y,
            temp_width,
            &format!(" {}", format_temp(temp, &config.temp_scale)),
            Style::default().fg(chrome::shade(
                &theme.temp,
                f64::from(temp * 100.0 / crit.max(1.0)),
            )),
        );
    }

    let gradient = chrome::plot_gradient(theme.cpu);
    let column_width = layout.label + layout.graph + PCT + if layout.temp { TEMP } else { 0 } + 1;
    let core_rows = inner.height.saturating_sub(2);
    for (index, core) in snapshot.cpu.cores.iter().enumerate() {
        let column = index / layout.rows;
        let row = (index % layout.rows) as u16;
        if row >= core_rows || column >= layout.columns {
            continue;
        }
        let x = inner.x + column as u16 * column_width;
        let y = inner.y + 1 + row;
        chrome::text(
            frame.buffer_mut(),
            x,
            y,
            layout.label,
            &format!("C{index}"),
            main,
        );
        if layout.graph > 0 {
            let history: Vec<f32> = snapshot
                .history
                .cpu
                .iter()
                .filter_map(|sample| sample.cores.get(index).map(|core| core.total))
                .collect();
            frame.render_widget(
                Graph::new(&history).no_zero(true).gradient(&gradient),
                Rect::new(x + layout.label, y, layout.graph, 1),
            );
        }
        let buffer = frame.buffer_mut();
        let pct_x = x + layout.label + layout.graph;
        let usage = f64::from(core.total);
        chrome::text(
            buffer,
            pct_x,
            y,
            PCT,
            &format!("{:>4.0}%", usage),
            Style::default().fg(chrome::shade(&theme.cpu, usage)),
        );
        if layout.temp {
            if let Some((_, temp, crit)) = snapshot
                .cpu
                .core_temperatures_c
                .iter()
                .find(|(core, _, _)| *core == index)
            {
                let crit = if *crit > 0.0 { *crit } else { 100.0 };
                chrome::text(
                    buffer,
                    pct_x + PCT,
                    y,
                    TEMP,
                    &format!(" {}", format_temp_short(*temp, &config.temp_scale)),
                    Style::default().fg(chrome::shade(&theme.temp, f64::from(temp * 100.0 / crit))),
                );
            }
        }
    }
    if inner.height >= 2 {
        let [one, five, fifteen] = snapshot.cpu.load_average;
        chrome::text_right(
            frame.buffer_mut(),
            inner.right(),
            inner.bottom() - 1,
            inner.x,
            &format!("Load avg: {one:.2} {five:.2} {fifteen:.2}"),
            main,
        );
    }
}

/// Core percentages as text rows, for a box too small for the sub-box.
fn draw_compact_cores(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    lines: u16,
    per_line: usize,
    snapshot: &ResourceSnapshot,
    view: &CpuView,
) {
    let theme = view.theme;
    let buffer = frame.buffer_mut();
    let main = Style::default().fg(theme.main_fg);
    for (index, core) in snapshot.cpu.cores.iter().enumerate() {
        let line = (index / per_line) as u16;
        if line >= lines {
            break;
        }
        let x = inner.x + (index % per_line) as u16 * COMPACT_CORE;
        let usage = f64::from(core.total);
        let x = chrome::text(buffer, x, y + line, 4, &format!("C{index:<3}"), main);
        chrome::text(
            buffer,
            x,
            y + line,
            COMPACT_CORE - 4,
            &format!("{usage:>3.0}%"),
            Style::default().fg(chrome::shade(&theme.cpu, usage)),
        );
    }
}

/// One-row CPU summary for a box too narrow for the core sub-box.
fn draw_summary(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    snapshot: &ResourceSnapshot,
    view: &CpuView,
) {
    let theme = view.theme;
    let main = Style::default().fg(theme.main_fg);
    let total = f64::from(snapshot.cpu.total.total);
    let [one, five, fifteen] = snapshot.cpu.load_average;
    let frequencies: Vec<u64> = snapshot
        .cpu
        .frequencies_mhz
        .iter()
        .flatten()
        .copied()
        .collect();
    let frequency = if frequencies.is_empty() || inner.width < 60 {
        String::new()
    } else {
        let average = frequencies.iter().sum::<u64>() / frequencies.len() as u64;
        format!("  {}", format_frequency(average, &view.config.freq_mode))
    };
    let load = format!("  Load avg {one:.1} {five:.1} {fifteen:.1}{frequency}");
    let load_width = if inner.width >= 40 {
        load.chars().count() as u16
    } else {
        0
    };
    let x = chrome::text(frame.buffer_mut(), inner.x, y, 4, "CPU ", main);
    let meter_width = inner.width.saturating_sub(4 + PCT + load_width);
    frame.render_widget(
        crate::ui::plot::Meter::new(
            total as f32,
            &chrome::plot_gradient(theme.cpu),
            theme.meter_bg,
        ),
        Rect::new(x, y, meter_width, 1),
    );
    let buffer = frame.buffer_mut();
    let x = chrome::text(
        buffer,
        x + meter_width,
        y,
        PCT,
        &format!("{:>4.0}%", total),
        Style::default().fg(chrome::shade(&theme.cpu, total)),
    );
    chrome::text(buffer, x, y, load_width, &load, main);
}

fn battery_spans(theme: &ResourcesTheme, percent: f32) -> Vec<Span<'static>> {
    let percent = f64::from(percent.clamp(0.0, 100.0));
    let filled = (percent / 10.0).round() as usize;
    let mut spans = vec![Span::styled(
        format!("BAT {percent:.0}% "),
        Style::default().fg(theme.title),
    )];
    for cell in 0..10 {
        let color = if cell < filled {
            chrome::shade(&theme.cpu, 100.0 - percent)
        } else {
            theme.meter_bg
        };
        spans.push(Span::styled("■", Style::default().fg(color)));
    }
    spans
}

/// Drop vendor boilerplate from a CPU brand string so it fits a tag.
fn shorten_model(model: &str, room: usize) -> String {
    let mut short = model.to_owned();
    for noise in [
        "(R)",
        "(TM)",
        "(tm)",
        "Intel ",
        "AMD ",
        "Apple ",
        "Core ",
        "Processor",
        "with Radeon Graphics",
    ] {
        short = short.replace(noise, "");
    }
    if let Some(at) = short.find('@') {
        short.truncate(at);
        short = short.trim_end().trim_end_matches("CPU").to_owned();
    }
    let short = short.split_whitespace().collect::<Vec<_>>().join(" ");
    short.chars().take(room).collect()
}

pub(super) fn format_frequency(mhz: u64, mode: &str) -> String {
    match mode {
        "mhz" => format!("{mhz} MHz"),
        "ghz" => format!("{:.1} GHz", mhz as f64 / 1000.0),
        _ if mhz >= 1000 => format!("{:.1} GHz", mhz as f64 / 1000.0),
        _ => format!("{mhz} MHz"),
    }
}

fn format_temp(celsius: f32, scale: &str) -> String {
    if scale == "fahrenheit" {
        format!("{:.0}°F", celsius * 1.8 + 32.0)
    } else {
        format!("{celsius:.0}°C")
    }
}

/// `45°` or `113°` without the unit letter, for a core column.
fn format_temp_short(celsius: f32, scale: &str) -> String {
    let value = if scale == "fahrenheit" {
        celsius * 1.8 + 32.0
    } else {
        celsius
    };
    format!("{value:>3.0}°")
}

#[cfg(test)]
mod tests;
