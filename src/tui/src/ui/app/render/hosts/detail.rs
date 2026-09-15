//! The selected host: what was declared, what was paired, and the command
//! to run on it.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::super::super::hosts::HostLiveness;
use super::super::super::types::App;
use super::super::color;

impl App {
    /// Draw the detail column.
    pub(super) fn draw_host_detail(&mut self, f: &mut Frame, area: Rect) {
        let Some(entry) = self.selected_host().cloned() else {
            let block = self.panel("Host");
            let inner = block.inner(area);
            f.render_widget(block, area);
            f.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from("Pick a host on the left, or press a to add one."),
                    Line::from(""),
                    Line::from("H switches the current host from any tab: the"),
                    Line::from("Sessions rail and the Workflows tab follow it."),
                ]))
                .wrap(Wrap { trim: false }),
                inner,
            );
            return;
        };
        let block = self.panel(entry.name.clone());
        let inner = block.inner(area);
        f.render_widget(block, area);

        let dim = Style::default().fg(ratatui::style::Color::DarkGray);
        let field = |label: &str, value: String| -> Line<'static> {
            Line::from(vec![
                Span::styled(format!("{label:<11}"), dim),
                Span::raw(value),
            ])
        };
        let section = &entry.section;
        let mut lines = vec![
            Line::from(vec![
                Span::styled("● ", Style::default().fg(color(entry.liveness.color()))),
                Span::styled(
                    entry.liveness.word().to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    match &entry.liveness {
                        HostLiveness::Failed(reason) => format!("  {reason}"),
                        _ => String::new(),
                    },
                    dim,
                ),
            ]),
            Line::from(""),
            field("address", section.host.clone()),
            field(
                "udp port",
                match section.udp_port {
                    0 => "not paired".to_string(),
                    port => port.to_string(),
                },
            ),
        ];
        if !section.udp_host.trim().is_empty() {
            lines.push(field("udp host", section.udp_host.clone()));
        }
        if !section.user.trim().is_empty() {
            lines.push(field("ssh user", section.user.clone()));
        }
        if section.port != 0 {
            lines.push(field("ssh port", section.port.to_string()));
        }
        if !section.identity_file.trim().is_empty() {
            lines.push(field("identity", section.identity_file.clone()));
        }
        if !section.workspace.trim().is_empty() {
            lines.push(field("workspace", section.workspace.clone()));
        }
        if let Some(state) = self.remote_hosts.get(&entry.id) {
            if !state.capabilities.version.is_empty() {
                lines.push(field("version", state.capabilities.version.clone()));
            }
            if !state.capabilities.harnesses.is_empty() {
                lines.push(field(
                    "offers",
                    state
                        .capabilities
                        .harnesses
                        .iter()
                        .map(|choice| choice.display_name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "⏎ connect · u use as current host · i issue a new key · e edit · d remove",
            dim,
        )));
        let text_height = lines.len() as u16;
        f.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }), inner);

        let Some(pending) = self.hosts.pending_key.clone() else {
            return;
        };
        if pending.host_id != entry.id {
            return;
        }
        // Boxed below the fields: the one thing on this page that has to be
        // copied exactly, drawn so its edges are obvious.
        let top = inner.y.saturating_add(text_height).saturating_add(1);
        if top + 5 > inner.y + inner.height {
            return;
        }
        let box_area = Rect {
            x: inner.x,
            y: top,
            width: inner.width,
            height: (inner.y + inner.height).saturating_sub(top).min(7),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .title(Span::styled(
                " Run this once on the host ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        let box_inner = block.inner(box_area);
        f.render_widget(block, box_area);
        let ssh_hint = if entry.section.host.trim().is_empty() {
            "y copies it again"
        } else {
            "y copies it again · s runs it over ssh"
        };
        f.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(Span::styled(
                    pending.command.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("Copied to the clipboard · {ssh_hint} · Esc hides it"),
                    dim,
                )),
            ]))
            .wrap(Wrap { trim: false }),
            box_inner,
        );
    }
}
