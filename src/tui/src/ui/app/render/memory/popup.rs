//! Centered detail modal for the selected Memory entry.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::super::super::types::{App, MemoryEntry};

impl App {
    /// Draw selected entry details over the Memory page instead of beside it.
    pub(super) fn draw_memory_detail_popup(&self, frame: &mut Frame, area: Rect) {
        let Some(entry) = self.selected_memory_entry() else {
            return;
        };
        let (title, mut body) = self.memory_detail(&entry);
        body.push(Line::from(""));
        body.push(Line::from(Span::styled(
            "Esc close",
            Style::default().add_modifier(Modifier::DIM),
        )));
        let popup = centered_rect(area, 70, 60);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(Text::from(body))
                .wrap(Wrap { trim: false })
                .block(self.panel(title)),
            popup,
        );
    }

    /// Build the detail title and body for a selected Memory entry.
    fn memory_detail(&self, entry: &MemoryEntry) -> (String, Vec<Line<'static>>) {
        let dim = Style::default().add_modifier(Modifier::DIM);
        match entry {
            MemoryEntry::Directive(text) => ("Directive".into(), vec![Line::from(text.clone())]),
            MemoryEntry::Facet { name, count } => (
                name.clone(),
                vec![
                    Line::from(format!("{count} observation(s) in this facet.")),
                    Line::from(Span::styled(
                        "Use Search to rank observations across facets.",
                        dim,
                    )),
                ],
            ),
            MemoryEntry::Hit(hit) => {
                let mut body = vec![Line::from(hit.text.clone()), Line::from("")];
                if let Some(quote) = &hit.quote {
                    body.push(Line::from(Span::styled(format!("“{quote}”"), dim)));
                    body.push(Line::from(""));
                }
                body.push(Line::from(Span::styled(
                    format!(
                        "facet {} · tier {} · score {:.3}",
                        hit.facet, hit.tier, hit.score
                    ),
                    dim,
                )));
                body.push(Line::from(Span::styled(hit.timestamp.clone(), dim)));
                (format!("{} · {}", hit.facet, hit.tier), body)
            }
        }
    }
}

/// Center a percentage-sized rectangle within `area`.
fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
