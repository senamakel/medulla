//! Persona-memory status summary shown before the focused data pages.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::ui::util::clip;

use super::super::super::types::App;

impl App {
    /// Draw workspace, pack, observation, directive, and facet status.
    pub(super) fn draw_memory_overview(&self, frame: &mut Frame, area: Rect) {
        let Some(status) = self.memory_status.as_ref() else {
            self.draw_memory_disabled(frame, area);
            return;
        };
        if !status.enabled {
            self.draw_memory_disabled(frame, area);
            return;
        }

        let facets = if status.facet_counts.is_empty() {
            "(none)".to_string()
        } else {
            status
                .facet_counts
                .iter()
                .map(|(name, count)| format!("{name}={count}"))
                .collect::<Vec<_>>()
                .join("  ")
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("● enabled", Style::default().fg(Color::Green)),
                Span::raw(format!(" · {}", clip(&status.workspace, 70))),
            ]),
            Line::from(if status.pack_exists {
                Span::styled(
                    format!("pack ● present · {}", clip(&status.pack_path, 72)),
                    Style::default().fg(Color::Green),
                )
            } else {
                Span::styled(
                    "pack ○ absent · open Maintenance to backfill",
                    Style::default().add_modifier(Modifier::DIM),
                )
            }),
            Line::from(format!(
                "{} observation(s) · {} directive(s)",
                status.entry_count, status.directives_count
            )),
            Line::from(Span::styled(
                format!("facets: {facets}"),
                Style::default().fg(self.theme.primary),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Open Directives, Facets, or Search to browse entries · r refresh",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .block(self.panel("Overview")),
            area,
        );
    }

    /// Explain how to enable persona memory when it is unavailable.
    pub(super) fn draw_memory_disabled(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(
                "Persona memory is not enabled.",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "Enable memory.enabled in config and provide an OpenRouter key.",
                Style::default().add_modifier(Modifier::DIM),
            )),
            Line::from(Span::styled(
                "Then open Maintenance to backfill the persona pack.",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: true })
                .block(self.panel("Persona memory")),
            area,
        );
    }
}
