//! Persona-pack refresh and paid ingest controls.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::super::super::types::App;

impl App {
    /// Draw memory workspace state and the paid ingest controls.
    pub(super) fn draw_memory_maintenance(&self, frame: &mut Frame, area: Rect) {
        let Some(status) = self.memory_status.as_ref() else {
            self.draw_memory_disabled(frame, area);
            return;
        };
        let state = if status.enabled {
            Span::styled("● enabled", Style::default().fg(Color::Green))
        } else {
            Span::styled("○ disabled", Style::default().fg(Color::Yellow))
        };
        let progress = if self.memory_ingesting {
            Line::from(Span::styled(
                "● ingesting… this can take a while",
                Style::default().fg(Color::Yellow),
            ))
        } else {
            Line::from(Span::styled(
                "b backfill everything · i ingest new activity · r refresh",
                Style::default().add_modifier(Modifier::DIM),
            ))
        };
        let lines = vec![
            Line::from(state),
            Line::from(format!("workspace: {}", status.workspace)),
            Line::from(format!(
                "persona pack: {}",
                if status.pack_exists {
                    status.pack_path.as_str()
                } else {
                    "absent"
                }
            )),
            Line::from(format!(
                "{} observation(s) · {} directive(s)",
                status.entry_count, status.directives_count
            )),
            Line::from(""),
            progress,
            Line::from(Span::styled(
                "Backfill and ingest may call a paid provider; duplicate runs are blocked.",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .block(self.panel("Maintenance")),
            area,
        );
    }
}
