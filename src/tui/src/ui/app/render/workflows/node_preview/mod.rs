//! Rich, persistent rendering of the workflow step under the graph cursor.
//!
//! The graph answers where a step sits; this pane answers what it will do.
//! Each common node kind gets a purpose-built presentation, while unknown
//! configuration remains inspectable as redacted, pretty-printed JSON.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use serde_json::Value;

use medulla::ui::workflows::{find_node_in, RunOverlay};
use medulla::workflows::{RunRecord, RunStatus};

use super::super::super::types::App;

mod kinds;

#[cfg(test)]
mod tests;

use kinds::{kind_lines, labelled_value};

impl App {
    /// Draw the selected node's useful contents below the workflow graph.
    pub(super) fn draw_workflow_node_preview(&mut self, f: &mut Frame, area: Rect) {
        let Some(selected) = self.selected_graph_node().cloned() else {
            return;
        };
        let run = self.selected_workflow_run().cloned();
        let run_title = run
            .as_ref()
            .map(|run| {
                format!(
                    " · run {} {}{}",
                    medulla::ui::workflows::rows::short_run_id(&run.id),
                    medulla::ui::workflows::status_label(run.status),
                    if run.status == RunStatus::Failed {
                        " · f fix via agent"
                    } else {
                        ""
                    }
                )
            })
            .unwrap_or_default();
        let block = crate::ui::widgets::panel(
            &self.theme,
            format!(
                "{} · {}{run_title} · wheel/Page scroll · i full",
                selected.name, selected.kind
            ),
            false,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.hit_workflow_preview = Some(inner);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let Some(config) = self
            .wf
            .graph
            .as_ref()
            .and_then(|graph| find_node_in(graph, &selected.id))
            .map(|node| node.config.clone())
        else {
            f.render_widget(
                Paragraph::new("Step changed on disk; press r to reload."),
                inner,
            );
            return;
        };

        let mut lines = Vec::new();
        if let Some(run) = &run {
            let state = RunOverlay::new(run).node(&selected.id);
            let duration = state
                .duration_ms
                .map(|ms| format!(" · {ms}ms"))
                .unwrap_or_default();
            lines.push(Line::from(Span::styled(
                format!("{} {}{duration}", state.state.glyph(), state.state.label()),
                Style::default().fg(super::super::color(state.state.color())),
            )));
            for diagnostic in state.diagnostics {
                lines.push(Line::from(Span::styled(
                    format!("  {diagnostic}"),
                    Style::default().fg(Color::Yellow),
                )));
            }
            lines.extend(run_lines(run, &selected.id, selected.kind == "agent"));
            lines.push(Line::from(""));
        }

        lines.push(connection_line(&selected.id, &self.workflow_layout().edges));
        lines.extend(kind_lines(&selected.kind, &config));
        let visible = inner.height as usize;
        let width = inner.width.max(1) as usize;
        let visual_lines = lines
            .iter()
            .map(|line| line.width().max(1).div_ceil(width))
            .sum::<usize>();
        self.wf.preview_scroll = self
            .wf
            .preview_scroll
            .min(visual_lines.saturating_sub(visible));
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .scroll((self.wf.preview_scroll.min(u16::MAX as usize) as u16, 0)),
            inner,
        );
    }
}

/// Render what the selected run recorded for one node.
fn run_lines(run: &RunRecord, node_id: &str, is_agent: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(summary) = &run.summary {
        lines.push(Line::from(vec![
            Span::styled("run  ", Style::default().add_modifier(Modifier::DIM)),
            Span::raw(summary.clone()),
        ]));
    }

    match run.steps.iter().rev().find(|step| step.node_id == node_id) {
        Some(step) => {
            if is_agent {
                match &step.input {
                    Some(input) => lines.extend(labelled_value("prompt", input)),
                    None => lines.push(Line::from(Span::styled(
                        "prompt  unavailable in this older run record",
                        Style::default().add_modifier(Modifier::DIM),
                    ))),
                }
            }
            if let Some(output) = &step.output {
                let label = if is_agent { "output" } else { "result" };
                let value = if is_agent {
                    agent_output(output)
                } else {
                    output.clone()
                };
                lines.extend(labelled_value(label, &value));
            } else if step.status.eq_ignore_ascii_case("error") {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}  no output was produced",
                        if is_agent { "output" } else { "result" }
                    ),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "result  unavailable in this older run record",
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
        }
        None => lines.push(Line::from(Span::styled(
            "○ not reached in this run",
            Style::default().fg(Color::DarkGray),
        ))),
    }

    if let Some(error) = &run.error {
        lines.extend(labelled_value("failure", &Value::String(error.clone())));
    }
    if let Some(diagnosis) = &run.diagnosis {
        for binding in diagnosis
            .null_bindings
            .iter()
            .filter(|binding| binding.node_id == node_id)
        {
            lines.push(warning_line(format!(
                "{} resolved to null · {}",
                binding.location, binding.suggestion
            )));
        }
        for hidden in diagnosis
            .hidden_errors
            .iter()
            .filter(|hidden| hidden.node_id == node_id)
        {
            lines.push(warning_line(hidden.message.clone().unwrap_or_else(|| {
                "error was swallowed by this step's policy".to_string()
            })));
        }
        if diagnosis.empty_prompts.iter().any(|id| id == node_id) {
            lines.push(warning_line("agent ran with an empty prompt".to_string()));
        }
        if let Some(skipped) = diagnosis
            .never_ran
            .iter()
            .find(|skipped| skipped.node_id == node_id)
        {
            lines.push(warning_line(match &skipped.routed_by {
                Some(condition) => format!("not reached; routed away by {condition}"),
                None => "not reached by this run".to_string(),
            }));
        }
    }
    if run.status == RunStatus::Failed {
        lines.push(Line::from(Span::styled(
            "[f] Fix this run via agent",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
    }
    lines
}

/// Pull the harness prose out of the engine's item envelope when available.
fn agent_output(output: &Value) -> Value {
    let replies = output
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("json")?.get("text")?.as_str())
        .map(|text| Value::String(text.to_string()))
        .collect::<Vec<_>>();
    match replies.len() {
        0 => output.clone(),
        1 => replies.into_iter().next().unwrap_or(Value::Null),
        _ => Value::Array(replies),
    }
}

/// A diagnosis line that stands apart from ordinary result data.
fn warning_line(message: String) -> Line<'static> {
    Line::from(Span::styled(
        format!("⚠ {message}"),
        Style::default().fg(Color::Yellow),
    ))
}

/// Summarize the selected node's incoming and outgoing graph connections.
fn connection_line(id: &str, edges: &[medulla::ui::workflows::PlacedEdge]) -> Line<'static> {
    let incoming = edges
        .iter()
        .filter(|edge| edge.to == id)
        .map(|edge| edge.from.as_str())
        .collect::<Vec<_>>();
    let outgoing = edges
        .iter()
        .filter(|edge| edge.from == id)
        .map(|edge| match &edge.label {
            Some(port) => format!("{port} → {}", edge.to),
            None => edge.to.clone(),
        })
        .collect::<Vec<_>>();
    let dim = Style::default().add_modifier(Modifier::DIM);
    Line::from(vec![
        Span::styled("flow  ", dim),
        Span::raw(if incoming.is_empty() {
            "entry".to_string()
        } else {
            incoming.join(", ")
        }),
        Span::styled("  →  ", dim),
        Span::raw(if outgoing.is_empty() {
            "output".to_string()
        } else {
            outgoing.join(", ")
        }),
    ])
}
