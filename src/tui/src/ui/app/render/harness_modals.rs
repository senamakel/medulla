//! The two overlays that own a harness handover: the picker that starts one,
//! and the question asked when the operator lets go of one.
//!
//! Both are centered popups over the content pane rather than strips under it,
//! because both are asked *about* something on screen — the picker names the
//! directory the rail is already showing, and the hand-back question is about
//! the pane immediately behind it.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::super::types::{App, HarnessPickerStep};

impl App {
    /// Draw the "start a harness" picker.
    pub(super) fn draw_harness_picker(&mut self, f: &mut Frame, area: Rect) {
        let Some(picker) = &self.harness_picker else {
            return;
        };
        let (rows, title) = match picker.step {
            HarnessPickerStep::Harness => (
                picker.choices.len(),
                "Choose harness — ↑/↓ · Enter workspace · Esc cancel",
            ),
            HarnessPickerStep::Workspace => (
                picker.workspace_choices.len(),
                "Choose workspace — type to filter · Tab complete · Enter start · Esc back",
            ),
        };
        let height = (rows as u16).saturating_add(7).clamp(8, 18);
        let area = centered(area, 62, height);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        // Cleared first: this floats over the rail, and without it the rows
        // underneath show through the gaps between provider names.
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        let mut lines =
            match picker.step {
                HarnessPickerStep::Harness => picker
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(index, choice)| {
                        let marker = if index == picker.index { "❯ " } else { "  " };
                        let style = if index == picker.index {
                            self.theme.selection()
                        } else {
                            Style::default()
                        };
                        TLine::from(Span::styled(
                            format!("{marker}{}", choice.display_name()),
                            style,
                        ))
                    })
                    .collect(),
                HarnessPickerStep::Workspace => {
                    let selected_harness = picker
                        .choices
                        .get(picker.index)
                        .map(|choice| choice.display_name())
                        .unwrap_or("harness");
                    let mut lines = vec![
                        TLine::from(Span::styled(
                            format!("  {selected_harness}"),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        TLine::from(format!(
                            "  search › {}▌",
                            medulla::ui::util::clip_left(&picker.workspace_query, 46)
                        )),
                        TLine::from(""),
                    ];
                    if picker.workspace_choices.is_empty() {
                        lines.push(TLine::from(Span::styled(
                            "  No matching folders",
                            Style::default().add_modifier(Modifier::DIM),
                        )));
                    }
                    lines.extend(picker.workspace_choices.iter().enumerate().map(
                        |(index, choice)| {
                            let marker = if index == picker.workspace_index {
                                "❯ "
                            } else {
                                "  "
                            };
                            let style = if index == picker.workspace_index {
                                self.theme.selection()
                            } else {
                                Style::default()
                            };
                            TLine::from(vec![
                                Span::styled(
                                    format!(
                                        "{marker}{}",
                                        medulla::ui::util::clip_left(&choice.path, 43)
                                    ),
                                    style,
                                ),
                                Span::styled(
                                    format!("  {}", choice.source),
                                    Style::default().add_modifier(Modifier::DIM),
                                ),
                            ])
                        },
                    ));
                    lines
                }
            };
        if picker.step == HarnessPickerStep::Harness {
            lines.push(TLine::from(""));
            lines.push(TLine::from(Span::styled(
                "  Next: choose a workspace",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        // Said here as well as in the status line, because it is the one fact
        // that makes this different from every other way to start a harness.
        lines.push(TLine::from(Span::styled(
            "  unmanaged · the orchestrator will not dispatch into it",
            Style::default().add_modifier(Modifier::DIM),
        )));
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draw the "you still hold this harness" question.
    pub(super) fn draw_handback_prompt(&mut self, f: &mut Frame, area: Rect) {
        let Some(prompt) = &self.handback_prompt else {
            return;
        };
        let area = centered(area, 66, 8);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .title(Span::styled(
                "You still have this harness",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        // An operator who typed /takecontrol made a decision; one who simply
        // focused in may not know they are holding anything. The sentence says
        // which of the two happened rather than implying the second.
        let how = if prompt.took_control {
            "You took this harness when you focused in."
        } else {
            "You asked for this harness."
        };
        let lines = vec![
            TLine::from(how),
            TLine::from("While you hold it, the orchestrator will not dispatch into it."),
            TLine::from(""),
            TLine::from(Span::styled(
                "Hand it back?  [Y] hand back · [N] keep it · [Esc] stay here",
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ];
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
    }
}

/// A `width` × `height` box centered in `area`, clamped to fit.
///
/// Both overlays are small and fixed-size: a percentage would make the
/// three-line question fill half a large terminal.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
