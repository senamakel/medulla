//! State, layout, keys, and rendering for the host Resources tab.

mod keys;
pub(super) mod layout;
#[cfg(test)]
mod tests;
mod types;

pub(super) use types::ResourcesState;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
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
                .style(Style::default().fg(Color::Yellow)),
                area,
            );
            return;
        }
        for (shown, rect, title, color) in [
            (state.shown.cpu, rects.cpu, "CPU", Color::Green),
            (state.shown.mem, rects.mem, "Mem", Color::Yellow),
            (state.shown.net, rects.net, "Net", Color::Blue),
            (state.shown.agents, rects.agents, "Agents", Color::Red),
        ] {
            if shown && rect.width > 0 && rect.height > 0 {
                let title = if title == "CPU" {
                    format!(
                        " CPU · {} · preset {} · {}ms ",
                        clock_text(),
                        state.preset + 1,
                        state.interval_ms
                    )
                } else {
                    format!(" {title} ")
                };
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(title, Style::default().fg(color))))
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(color)),
                        ),
                    rect,
                );
            }
        }
    }
}

fn clock_text() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        % 86_400;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}
