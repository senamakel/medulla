//! The `H` overlay: a short list of machines, drawn over whatever tab is up.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::super::super::types::App;
use super::super::color;

impl App {
    /// Draw the host switcher.
    pub(in crate::ui::app::render) fn draw_host_switcher(&mut self, f: &mut Frame, area: Rect) {
        let Some(switcher) = &self.host_switcher else {
            return;
        };
        let height = (switcher.rows.len() as u16).saturating_add(4).clamp(6, 16);
        let width = 48.min(area.width);
        let area = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height: height.min(area.height),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .title(Span::styled(
                " Switch host — ⏎ select · H next · Esc close ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);
        let mut lines = Vec::new();
        for (index, (selection, label, liveness)) in switcher.rows.iter().enumerate() {
            let mut style = Style::default();
            if index == switcher.index {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let current = if *selection == self.current_host { "▸" } else { " " };
            lines.push(Line::from(vec![
                Span::styled(format!("{current} "), style),
                Span::styled("● ", style.fg(color(liveness.color()))),
                Span::styled(label.clone(), style),
                Span::styled(
                    match selection {
                        super::super::super::hosts::HostSelection::Host(_) => {
                            format!("  {}", liveness.word())
                        }
                        _ => String::new(),
                    },
                    style.fg(ratatui::style::Color::DarkGray),
                ),
            ]));
        }
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
    }
}
