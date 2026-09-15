//! The Hosts tab's list: one row per machine, coloured by how it is doing.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::super::super::types::App;
use super::super::color;

impl App {
    /// Draw the list column.
    pub(super) fn draw_host_list(&mut self, f: &mut Frame, area: Rect) {
        let count = self.hosts.entries.len();
        let block = self.content_panel(format!("Hosts · {count}"));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if count == 0 {
            let lines = vec![
                Line::from(Span::styled(
                    "No hosts yet",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("a adds a machine. You will get a one-line"),
                Line::from("command to run on it, once; after that it"),
                Line::from("is reachable from here without SSH."),
            ];
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
            return;
        }
        let mut lines = Vec::new();
        let current = self.current_host.host_id().map(str::to_string);
        for (index, entry) in self.hosts.entries.iter().enumerate() {
            let mut style = Style::default();
            if index == self.hosts.index {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let marker = if current.as_deref() == Some(entry.id.as_str()) {
                "▸"
            } else {
                " "
            };
            let sessions = match entry.sessions {
                0 => String::new(),
                1 => " · 1 session".to_string(),
                n => format!(" · {n} sessions"),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled("● ", style.fg(color(entry.liveness.color()))),
                Span::styled(entry.name.clone(), style.add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  {}{sessions}", entry.liveness.word()),
                    style.fg(self.theme.muted),
                ),
            ]));
        }
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
    }
}
