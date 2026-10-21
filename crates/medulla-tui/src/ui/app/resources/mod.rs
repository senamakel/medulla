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
                        " CPU [1] · {} · menu · preset {} · {}ms +/- ",
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
    }
}

fn clock_text() -> String {
    chrono::Local::now().format("%X").to_string()
}
