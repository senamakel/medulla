//! The workflow run under the Agents cursor, drawn where its session's
//! transcript would be.
//!
//! A run started over MCP executes in another process entirely, so until this
//! existed the rail was the only place it appeared at all — one line, and
//! selecting it showed the terminal of the session that started it. The line
//! said a run existed and nothing else could be asked of it.
//!
//! This draws the run itself, and it does so with the **Workflows tab's own**
//! canvas rather than a second rendering of the same facts: the graph with the
//! run overlaid on it, and under that the step preview with the harness output
//! the working step is producing. Two renderings of a run would drift, and the
//! one an operator learns on the Workflows tab is the one they should find here.
//!
//! The mirror that makes that possible — pointing the workflow state at this run
//! — is set up in [`super::mirror_selected_workflow_run`], because it must
//! happen before the layout, not during the draw.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::super::super::rail::WorkflowRunRailRow;
use super::super::super::types::App;
use super::super::color;

/// Count terminal rows a set of paragraph lines occupies after word wrapping.
///
/// This mirrors Ratatui's word packing: words share a row when their separating
/// space and display widths fit, while oversized words consume as many rows as
/// needed. Counting every word separately would wrongly spend a whole row on
/// each short word and hide live progress in an otherwise roomy pane.
fn wrapped_rows(lines: &[TLine<'_>], width: u16) -> usize {
    let width = usize::from(width).max(1);
    lines
        .iter()
        .map(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            text.lines()
                .map(|paragraph| {
                    let mut rows = 1;
                    let mut used = 0;
                    for word in paragraph.split_whitespace() {
                        let word_width = word.width().max(1);
                        if used == 0 {
                            rows += word_width.saturating_sub(1) / width;
                            used = word_width % width;
                            if used == 0 {
                                used = width;
                            }
                        } else if used + 1 + word_width <= width {
                            used += 1 + word_width;
                        } else {
                            rows += 1 + word_width.saturating_sub(1) / width;
                            used = word_width % width;
                            if used == 0 {
                                used = width;
                            }
                        }
                    }
                    rows
                })
                .sum::<usize>()
                .max(1)
        })
        .sum()
}

impl App {
    /// Draw the selected workflow run into `area`.
    ///
    /// Delegates to the shared canvas whenever this machine has the workflow
    /// installed. When it does not there is no graph to overlay anything onto,
    /// so the pane says what it does know instead of drawing an empty box — see
    /// [`draw_uninstalled_run`](Self::draw_uninstalled_run).
    pub(super) fn draw_agents_workflow_run(
        &mut self,
        f: &mut Frame,
        area: Rect,
        row: &WorkflowRunRailRow,
    ) {
        if self.workflows.iter().any(|w| w.id == row.run.workflow_id) {
            self.draw_workflow_canvas(f, area);
            return;
        }
        self.draw_uninstalled_run(f, area, row);
    }

    /// What the pane says for a run whose workflow this machine does not have.
    ///
    /// Ordinary rather than exceptional: a session working in another checkout
    /// reports runs of workflows installed *there*. The graph is unavailable, but
    /// the status, the timings and the frames the run has streamed are not — and
    /// those are most of what the pane would have shown anyway.
    fn draw_uninstalled_run(&mut self, f: &mut Frame, area: Rect, row: &WorkflowRunRailRow) {
        let run = &row.run;
        let block = crate::ui::widgets::panel(
            &self.theme,
            format!("⚙ {} · {}", run.workflow_id, run.status.label()),
            false,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let dim = Style::default().add_modifier(Modifier::DIM);
        let mut lines = vec![
            TLine::from(vec![
                Span::styled("status  ", dim),
                Span::styled(
                    run.status.label().to_string(),
                    Style::default().fg(color(run.status.color())),
                ),
            ]),
            TLine::from(vec![
                Span::styled("running ", dim),
                Span::raw(super::rail::workflow_run_elapsed(
                    run,
                    medulla::clock::now_millis(),
                )),
            ]),
            TLine::from(vec![
                Span::styled("started ", dim),
                Span::raw(format!(
                    "by session {}",
                    medulla::ui::workflows::short_session(&row.session_id)
                )),
            ]),
            TLine::from(""),
            TLine::from(Span::styled(
                format!(
                    "'{}' is not installed on this machine, so there is no graph \
                     to draw. The session above started it and has it; what \
                     follows is what the run has reported.",
                    run.workflow_id
                ),
                dim,
            )),
            TLine::from(""),
        ];

        // The same frame vocabulary the step preview uses, so a run read here and
        // a run read on the Workflows tab look like the same kind of thing.
        let frames: Vec<String> = run.frames.iter().map(|frame| frame.text.clone()).collect();
        let live_lines =
            crate::ui::app::render::workflows::live_lines(&frames, !run.status.is_terminal());
        // This pane has no independent scroll state. Keep its context, then
        // spend the remaining rows on the newest reported progress: the reason
        // an operator opens a live run is to see what it is doing now.
        let live_rows = (inner.height as usize).saturating_sub(wrapped_rows(&lines, inner.width));
        if live_rows > 0 && !live_lines.is_empty() {
            let header_rows = wrapped_rows(&live_lines[..1], inner.width);
            let mut tail_rows = live_rows;
            if header_rows <= tail_rows {
                lines.push(live_lines[0].clone());
                tail_rows -= header_rows;
            }
            let mut tail = Vec::new();
            for line in live_lines[1..].iter().rev() {
                let rows = wrapped_rows(std::slice::from_ref(line), inner.width);
                if rows > tail_rows && !tail.is_empty() {
                    break;
                }
                tail.push(line.clone());
                tail_rows = tail_rows.saturating_sub(rows);
            }
            lines.extend(tail.into_iter().rev());
        }

        f.render_widget(
            Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
            inner,
        );
    }
}
