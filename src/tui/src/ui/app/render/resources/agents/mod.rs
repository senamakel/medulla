//! The Agents box: every coding-agent process on this host and on each
//! connected remote host, as a btop-style process table.

use std::cmp::Ordering;

use crate::ui::resources::sampler::{ProcessSnapshot, ResourceSnapshot};
use medulla::protocol::HarnessProvider;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::ui::app::resources::chrome::{self, Bar};
use crate::ui::app::resources::types::{AgentIdentity, AgentSignal, AgentSort, ResourcesState};
use crate::ui::theme::ResourcesTheme;

mod command;
mod rows;
#[cfg(test)]
mod tests;
mod types;

use command::display_command;
use rows::kind_counts;
pub(in crate::ui::app) use rows::{rows, visible_rows_with_remote};
pub(in crate::ui::app) use types::AgentRow;

pub(in crate::ui::app) fn draw_agents(
    frame: &mut Frame,
    rect: Rect,
    snapshot: &ResourceSnapshot,
    remote_hosts: &[(String, Vec<medulla::protocol::RemoteSessionRow>)],
    state: &mut ResourcesState,
    theme: &ResourcesTheme,
) {
    let rows = visible_rows_with_remote(snapshot, remote_hosts, state);
    if let Some(following) = state.agent_follow.clone() {
        if !snapshot.processes.iter().any(|process| {
            (process.pid, process.start_time) == (following.pid, following.started)
                && following.host.is_none()
        }) {
            state.agent_follow = None;
            if state.agent_selected.as_ref() == Some(&following) {
                state.agent_selected = None;
            }
        } else if rows.iter().any(|row| row.identity() == following) {
            state.agent_selected = Some(following.clone());
        }
    }
    let now = std::time::Instant::now();
    let sample_due = state
        .agent_last_sample
        .is_none_or(|last| now.duration_since(last).as_millis() >= u128::from(state.interval_ms));
    let current: std::collections::HashSet<_> = rows.iter().map(AgentRow::identity).collect();
    state
        .agent_cpu_history
        .retain(|identity, _| current.contains(identity));
    if sample_due {
        for row in &rows {
            let history = state.agent_cpu_history.entry(row.identity()).or_default();
            history.push(row.cpu);
            if history.len() > 24 {
                history.remove(0);
            }
        }
        state.agent_last_sample = Some(now);
    }
    let area = draw_chrome(frame, rect, &rows, state, theme);
    if area.is_empty() {
        return;
    }
    if state.agent_detail {
        super::agents_detail::draw(
            frame,
            area,
            rows.iter()
                .find(|r| Some(r.identity()) == state.agent_selected),
            theme,
        );
        draw_signal_confirmation(frame, area, snapshot, state, theme);
        return;
    }
    let selected = state.agent_selected.clone();
    let compact = area.width < 88;
    let header = if compact {
        Row::new(["Pid:", "Agent / Origin:", "Cpu%", "MemB", "Run"])
    } else {
        Row::new([
            "Pid:",
            "Agent:",
            "Origin:",
            "Workspace:",
            "Th:",
            "MemB",
            "Cpu%",
            "",
            "Runtime",
        ])
    };
    let main = Style::default().fg(theme.main_fg);
    let dim = Style::default().fg(theme.inactive_fg);
    let agent_table_rows: Vec<Row> = rows
        .iter()
        .map(|row| {
            let history = state.agent_cpu_history.get(&row.identity());
            let graph = history
                .map(|history| mini_graph(history))
                .unwrap_or_default();
            let cpu = Style::default().fg(chrome::shade(&theme.process, row.cpu));
            let workspace = shorten(&row.workspace, if compact { 18 } else { 25 });
            let origin = if row.external {
                "external".to_owned()
            } else {
                row.origin.clone()
            };
            let tree = format!(
                "{}{}",
                "  ".repeat(row.depth),
                if row.depth > 0 { "└─ " } else { "" }
            );
            let cells: Vec<Cell> = if compact {
                vec![
                    Cell::from(row.pid.to_string()).style(dim),
                    Cell::from(Line::from(vec![
                        Span::styled(tree, dim),
                        Span::styled(row.kind.clone(), main.add_modifier(Modifier::BOLD)),
                        Span::styled(format!(" · {origin}"), main),
                    ])),
                    Cell::from(format!("{:>5.1}", row.cpu)).style(cpu),
                    Cell::from(format!("{:>6}", bytes(row.memory))).style(main),
                    Cell::from(duration(row.runtime)).style(main),
                ]
            } else {
                vec![
                    Cell::from(row.pid.to_string()).style(dim),
                    Cell::from(Line::from(vec![
                        Span::styled(tree, dim),
                        Span::styled(row.kind.clone(), main.add_modifier(Modifier::BOLD)),
                    ])),
                    Cell::from(origin).style(if row.external { dim } else { main }),
                    Cell::from(workspace).style(main),
                    Cell::from(format!("{:>3}", row.threads)).style(main),
                    Cell::from(format!("{:>6}", bytes(row.memory))).style(main),
                    Cell::from(format!("{:>5.1}", row.cpu)).style(cpu),
                    Cell::from(graph).style(cpu),
                    Cell::from(format!("{:>7}", duration(row.runtime))).style(main),
                ]
            };
            let mut item = Row::new(cells);
            if Some(row.identity()) == selected {
                item = item.style(
                    Style::default()
                        .bg(theme.selected_bg)
                        .fg(theme.selected_fg)
                        .add_modifier(Modifier::BOLD),
                );
            }
            item
        })
        .collect();
    let mut table_rows = Vec::new();
    let mut display_indices = Vec::with_capacity(rows.len());
    let mut last_remote_host: Option<&str> = None;
    for (row, item) in rows.iter().zip(agent_table_rows) {
        if let Some(host) = row.host.as_deref() {
            if last_remote_host != Some(host) {
                table_rows.push(Row::new(vec![
                    Cell::from("Remote").style(Style::default().fg(theme.hi_fg)),
                    Cell::from(host.to_owned()).style(
                        Style::default()
                            .fg(theme.title)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            last_remote_host = Some(host);
        } else {
            last_remote_host = None;
        }
        display_indices.push(table_rows.len());
        table_rows.push(item);
    }
    let widths = if compact {
        vec![
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Min(17),
            ratatui::layout::Constraint::Length(5),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(5),
        ]
    } else {
        vec![
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Min(12),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(5),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(7),
        ]
    };
    let table = Table::new(table_rows, widths).header(
        header.style(
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ),
    );
    let selected_index = selected.as_ref().and_then(|identity| {
        rows.iter()
            .position(|row| row.identity() == *identity)
            .map(|index| display_indices[index])
    });
    let mut table_state = ratatui::widgets::TableState::default();
    table_state.select(selected_index);
    frame.render_stateful_widget(table, area, &mut table_state);
    if rows.is_empty() {
        let message = if state.agent_filter.is_some() {
            "No agents match the filter"
        } else {
            "No agent processes running"
        };
        chrome::text(
            frame.buffer_mut(),
            area.x + 1,
            area.y + 2.min(area.height.saturating_sub(1)),
            area.width.saturating_sub(2),
            message,
            dim,
        );
    }
    draw_signal_confirmation(frame, area, snapshot, state, theme);
}

/// Draw the Agents frame and its tags, and return the area inside it.
pub(super) fn draw_chrome(
    frame: &mut Frame,
    rect: Rect,
    rows: &[AgentRow],
    state: &mut ResourcesState,
    theme: &ResourcesTheme,
) -> Rect {
    let buffer = frame.buffer_mut();
    let area = chrome::frame(buffer, rect, theme.proc_box);
    let mut top = Bar::top(rect, theme.proc_box);
    state.hits.agents_toggle = top.left(buffer, &chrome::numbered(theme, 4, "agents"));
    let filter = match (&state.agent_filter, state.agent_filter_editing) {
        (filter, true) => {
            let mut spans = chrome::keyed(theme, "f ", 'f', true);
            spans.push(Span::styled(
                format!("{}█", filter.as_deref().unwrap_or_default()),
                Style::default().fg(theme.title),
            ));
            spans
        }
        (Some(filter), false) => {
            let mut spans = chrome::keyed(theme, "f ", 'f', true);
            spans.push(Span::styled(
                filter.clone(),
                Style::default().fg(theme.title),
            ));
            spans
        }
        (None, false) => chrome::keyed(theme, "filter", 'f', false),
    };
    top.left(buffer, &filter);
    // Following changes what the selection does, so it outranks the sort
    // and view toggles when the border runs out of room.
    if let Some(following) = &state.agent_follow {
        top.right(
            buffer,
            &chrome::keyed(theme, &format!("Follow {}", following.pid), 'F', true),
        );
    }
    top.right(
        buffer,
        &chrome::stepper(theme, "←", sort_label(state.agent_sort), "→"),
    );
    top.right(buffer, &chrome::keyed(theme, "tree", 'e', state.agent_tree));
    top.right(
        buffer,
        &chrome::keyed(theme, "reverse", 'r', state.agent_reverse),
    );

    let mut bottom = Bar::bottom(rect, theme.proc_box);
    let selected = state.agent_selected.is_some();
    bottom.left(buffer, &chrome::stepper(theme, "↑", "select", "↓"));
    bottom.left(buffer, &chrome::keyed(theme, "info ↵", '↵', selected));
    bottom.left(buffer, &chrome::keyed(theme, "terminate", 't', selected));
    bottom.left(buffer, &chrome::keyed(theme, "kill", 'k', selected));
    bottom.left(buffer, &chrome::keyed(theme, "stop", 's', selected));
    let position = state
        .agent_selected
        .as_ref()
        .and_then(|identity| rows.iter().position(|row| row.identity() == *identity))
        .map_or(0, |index| index + 1);
    bottom.right(
        buffer,
        &chrome::plain(theme, &format!("{position}/{}", rows.len())),
    );
    let summary = format!(
        "{} · cpu {:.1}% · {}",
        kind_counts(rows),
        rows.iter().map(|r| r.cpu).sum::<f64>(),
        bytes(rows.iter().map(|r| r.memory).sum::<u64>())
    );
    if !rows.is_empty() {
        top.center(buffer, &chrome::plain(theme, &summary));
    }
    area
}

pub(super) fn sort_label(sort: AgentSort) -> &'static str {
    match sort {
        AgentSort::Pid => "pid",
        AgentSort::Kind => "agent",
        AgentSort::Origin => "origin",
        AgentSort::Workspace => "workspace",
        AgentSort::Threads => "threads",
        AgentSort::Memory => "memory",
        AgentSort::Cpu => "cpu",
        AgentSort::Runtime => "runtime",
    }
}

pub(super) fn draw_signal_confirmation(
    frame: &mut Frame,
    area: Rect,
    snapshot: &ResourceSnapshot,
    state: &ResourcesState,
    theme: &ResourcesTheme,
) {
    let Some(armed) = state.agent_signal_armed else {
        return;
    };
    let row = rows(snapshot)
        .into_iter()
        .find(|row| row.pid == armed.pid && row.started == armed.started);
    let process = snapshot
        .processes
        .iter()
        .find(|process| process.pid == armed.pid && process.start_time == armed.started);
    let kind = row.as_ref().map_or("Unknown", |row| row.kind.as_str());
    let origin = row.as_ref().map_or("external", |row| row.origin.as_str());
    let command = process.map_or_else(|| "unavailable".into(), display_command);
    let signal = match armed.signal {
        AgentSignal::Terminate => "terminate",
        AgentSignal::Kill => "kill",
        AgentSignal::Stop => "stop",
    };
    let external = row.as_ref().is_some_and(|row| row.external);
    let instruction = match (external, armed.stage) {
        (true, crate::ui::app::resources::types::AgentSignalConfirmStage::First) => {
            "Press y for the second confirmation; Esc cancels"
        }
        (true, crate::ui::app::resources::types::AgentSignalConfirmStage::ExternalSecond) => {
            "External target: press uppercase Y to send; Esc cancels"
        }
        (false, _) => "Press y to send; Esc cancels",
    };
    let prompt = format!(
        "Kind: {kind}\nPID: {}\nCommand: {command}\nOrigin: {origin}\nSignal: {signal}\nTarget host: local\n\n{instruction}",
        armed.pid
    );
    let popup = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2).min(10),
    );
    frame.render_widget(Clear, popup);
    chrome::fill(frame.buffer_mut(), popup, theme.main_bg);
    frame.render_widget(
        Paragraph::new(prompt).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(if external {
                    " EXTERNAL PROCESS · Confirm signal "
                } else {
                    " Confirm process signal "
                })
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        ),
        popup,
    );
}

pub(super) fn bytes(value: u64) -> String {
    if value >= 1 << 30 {
        format!("{:.1}G", value as f64 / (1u64 << 30) as f64)
    } else if value >= 1 << 20 {
        format!("{:.0}M", value as f64 / (1u64 << 20) as f64)
    } else {
        format!("{}K", value / 1024)
    }
}
pub(super) fn duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h", seconds / 3600)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}
pub(super) fn shorten(value: &str, max: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        value.to_owned()
    } else {
        format!(
            "…{}",
            chars[chars.len().saturating_sub(max.saturating_sub(1))..]
                .iter()
                .collect::<String>()
        )
    }
}
pub(super) fn mini_graph(values: &[f64]) -> String {
    let values: Vec<f32> = values.iter().map(|value| *value as f32).collect();
    crate::ui::plot::sparkline(&values, 8)
}
