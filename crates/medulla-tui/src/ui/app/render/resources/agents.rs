use std::cmp::Ordering;

use medulla::protocol::HarnessProvider;
use medulla_monitor::sampler::{ProcessSnapshot, ResourceSnapshot};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::ui::app::resources::types::{AgentSignal, AgentSort, ResourcesState};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::ui::app) struct AgentRow {
    pub pid: u32,
    pub started: u64,
    pub kind: String,
    pub origin: String,
    pub workspace: String,
    pub threads: usize,
    pub memory: u64,
    pub cpu: f64,
    pub runtime: u64,
    pub ppid: Option<u32>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub external: bool,
}

pub(super) fn rows(snapshot: &ResourceSnapshot) -> Vec<AgentRow> {
    let registrations = medulla_monitor::agents::agent_registry().snapshot();
    snapshot
        .processes
        .iter()
        .filter_map(|process| {
            let registered = registrations.iter().find(|entry| {
                entry.pid == process.pid && entry.started_at_secs == process.start_time
            });
            let detected = detect_kind(process);
            let kind = registered
                .map(|entry| pretty_kind(&entry.provider))
                .or(detected)?;
            let (origin, session_id, task_id, external) = match registered {
                Some(entry) if entry.session_id.is_some() => (
                    entry.session_id.clone().unwrap_or_default(),
                    entry.session_id.clone(),
                    entry.task_id.clone(),
                    false,
                ),
                Some(entry) if entry.task_id.is_some() => (
                    format!("task {}", entry.task_id.as_deref().unwrap_or_default()),
                    None,
                    entry.task_id.clone(),
                    false,
                ),
                Some(_) => ("host".into(), None, None, false),
                None => ("external".into(), None, None, true),
            };
            Some(AgentRow {
                pid: process.pid,
                started: process.start_time,
                kind,
                origin,
                workspace: process.cwd.clone().unwrap_or_else(|| "—".into()),
                threads: process.threads,
                memory: process.memory_bytes,
                cpu: process.cpu_percent,
                runtime: (medulla::clock::now_millis()
                    - (process.start_time.saturating_mul(1000) as i64))
                    .max(0) as u64
                    / 1000,
                ppid: process.ppid,
                session_id,
                task_id,
                external,
            })
        })
        .collect()
}

fn detect_kind(process: &ProcessSnapshot) -> Option<String> {
    let full = format!(
        "{} {}",
        process.exe.as_deref().unwrap_or_default(),
        process.cmd.join(" ")
    )
    .to_lowercase();
    if full.contains("codex-acp") {
        Some("Codex-ACP".into())
    } else if full.contains("claude") {
        Some("Claude".into())
    } else if full.contains("openhuman") || full.contains("medulla") {
        Some("OpenHuman".into())
    } else if full.contains("aider")
        || full.contains("gemini")
        || full.contains("goose")
        || full.contains("cursor")
        || full.contains("opencode")
    {
        Some("External".into())
    } else {
        None
    }
}

fn pretty_kind(provider: &str) -> String {
    match HarnessProvider::from_wire(provider) {
        Some(HarnessProvider::Claude) => "Claude".into(),
        Some(HarnessProvider::Codex) => "Codex".into(),
        Some(HarnessProvider::Opencode) => "OpenCode".into(),
        Some(_) => provider.to_owned(),
        None if provider.to_lowercase().contains("openhuman") => "OpenHuman".into(),
        None if provider.to_lowercase().contains("codex-acp") => "Codex-ACP".into(),
        None if provider.to_lowercase().contains("claude") => "Claude".into(),
        None => provider.to_owned(),
    }
}

pub(in crate::ui::app) fn visible_rows(
    snapshot: &ResourceSnapshot,
    state: &ResourcesState,
) -> Vec<AgentRow> {
    let mut rows = rows(snapshot);
    if let Some(filter) = state.agent_filter.as_deref() {
        let (regex, pattern) = filter
            .strip_prefix('!')
            .map_or((false, filter), |pattern| (true, pattern));
        if regex {
            if let Ok(pattern) = regex::Regex::new(&format!("(?i){pattern}")) {
                rows.retain(|row| pattern.is_match(&row.kind));
            } else {
                rows.clear();
            }
        } else {
            let pattern = pattern.to_lowercase();
            rows.retain(|row| {
                format!("{} {} {} {}", row.pid, row.kind, row.origin, row.workspace)
                    .to_lowercase()
                    .contains(&pattern)
            });
        }
    }
    rows.sort_by(|a, b| compare(a, b, state.agent_sort));
    if state.agent_reverse {
        rows.reverse();
    }
    rows
}

fn compare(a: &AgentRow, b: &AgentRow, key: AgentSort) -> Ordering {
    match key {
        AgentSort::Pid => a.pid.cmp(&b.pid),
        AgentSort::Kind => a.kind.cmp(&b.kind),
        AgentSort::Origin => a.origin.cmp(&b.origin),
        AgentSort::Workspace => a.workspace.cmp(&b.workspace),
        AgentSort::Threads => a.threads.cmp(&b.threads),
        AgentSort::Memory => a.memory.cmp(&b.memory),
        AgentSort::Cpu => a.cpu.total_cmp(&b.cpu),
        AgentSort::Runtime => a.runtime.cmp(&b.runtime),
    }
}

pub(in crate::ui::app) fn draw_agents(
    frame: &mut Frame,
    area: Rect,
    snapshot: &ResourceSnapshot,
    state: &mut ResourcesState,
) {
    let rows = visible_rows(snapshot, state);
    for row in &rows {
        let history = state
            .agent_cpu_history
            .entry((row.pid, row.started))
            .or_default();
        history.push(row.cpu);
        if history.len() > 24 {
            history.remove(0);
        }
    }
    if state.agent_selected.is_none() {
        state.agent_selected = rows.first().map(|r| (r.pid, r.started));
    }
    if state.agent_follow {
        state.agent_selected = rows.first().map(|r| (r.pid, r.started));
    }
    if state.agent_detail {
        super::agents_detail::draw(
            frame,
            area,
            rows.iter()
                .find(|r| Some((r.pid, r.started)) == state.agent_selected),
        );
        return;
    }
    let selected = state.agent_selected;
    let compact = area.width < 105;
    let header = if compact {
        Row::new(["PID", "Agent / Origin", "CPU", "Mem", "Run"])
    } else {
        Row::new([
            "PID",
            "Agent",
            "Origin",
            "Workspace",
            "Th",
            "Mem",
            "CPU%",
            "CPU",
            "Runtime",
        ])
    };
    let table_rows: Vec<Row> = rows
        .iter()
        .map(|row| {
            let graph = state
                .agent_cpu_history
                .get(&(row.pid, row.started))
                .map(|history| mini_graph(history))
                .unwrap_or_default();
            let workspace = shorten(&row.workspace, if compact { 18 } else { 25 });
            let origin = if row.external {
                "external".to_owned()
            } else {
                row.origin.clone()
            };
            let cells = if compact {
                vec![
                    row.pid.to_string(),
                    format!("{} · {}", row.kind, origin),
                    format!("{:.1}", row.cpu),
                    bytes(row.memory),
                    duration(row.runtime),
                ]
            } else {
                vec![
                    row.pid.to_string(),
                    row.kind.clone(),
                    origin,
                    workspace,
                    row.threads.to_string(),
                    bytes(row.memory),
                    format!("{:.1}", row.cpu),
                    graph,
                    duration(row.runtime),
                ]
            };
            let mut item = Row::new(cells);
            if Some((row.pid, row.started)) == selected {
                item = item.style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            }
            item
        })
        .collect();
    let title = format!(" Agents · {} · {} · CPU {:.1}% · Mem {} · ←/→ sort r reverse e tree f filter Enter detail F follow t/k/s signal ",
            rows.len(), kind_counts(&rows), rows.iter().map(|r| r.cpu).sum::<f64>(), bytes(rows.iter().map(|r| r.memory).sum()));
    let widths = if compact {
        vec![
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Min(17),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(6),
        ]
    } else {
        vec![
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Min(12),
            ratatui::layout::Constraint::Length(4),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(7),
        ]
    };
    let table = Table::new(table_rows, widths)
        .header(header.style(Style::default().add_modifier(Modifier::BOLD)))
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(table, area);
    if let Some((pid, _, signal)) = state.agent_signal_armed {
        let row = rows.iter().find(|row| row.pid == pid);
        if let Some(row) = row {
            let label = if row.external {
                "EXTERNAL PROCESS"
            } else {
                "Medulla agent"
            };
            let signal = match signal {
                AgentSignal::Terminate => "terminate",
                AgentSignal::Kill => "kill",
                AgentSignal::Stop => "stop",
            };
            let prompt = format!("{label}: {signal} PID {pid}? y confirm · any other key cancels");
            frame.render_widget(
                Paragraph::new(Line::from(prompt)).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1),
            );
        }
    }
    if state.agent_filter_editing {
        let filter = state.agent_filter.as_deref().unwrap_or_default();
        frame.render_widget(
            Paragraph::new(format!("Filter: {filter}_   Enter apply · Esc cancel")),
            Rect::new(
                area.x + 1,
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(2),
                1,
            ),
        );
    }
}

fn kind_counts(rows: &[AgentRow]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.kind.as_str()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|(kind, count)| format!("{kind} {count}"))
        .collect::<Vec<_>>()
        .join(" · ")
}
fn bytes(value: u64) -> String {
    if value >= 1 << 30 {
        format!("{:.1}G", value as f64 / (1u64 << 30) as f64)
    } else if value >= 1 << 20 {
        format!("{:.0}M", value as f64 / (1u64 << 20) as f64)
    } else {
        format!("{}K", value / 1024)
    }
}
fn duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h", seconds / 3600)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}
fn shorten(value: &str, max: usize) -> String {
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
fn mini_graph(values: &[f64]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    values
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|v| BARS[(v.clamp(0.0, 100.0) / 100.0 * 7.0).round() as usize])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::resources::types::ResourcesState;

    fn process(pid: u32, command: &str, cpu: f64, mem: u64) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            ppid: None,
            exe: Some(command.into()),
            cmd: vec![command.into()],
            cwd: Some(format!("/workspace/{command}")),
            start_time: 100,
            cpu_percent: cpu,
            memory_bytes: mem,
            disk_read_bytes: 0,
            disk_written_bytes: 0,
            threads: pid as usize,
        }
    }
    fn snapshot() -> ResourceSnapshot {
        ResourceSnapshot::default().with_processes(vec![
            process(10, "claude", 10.0, 4000),
            process(20, "codex-acp", 20.0, 3000),
            process(30, "openhuman", 30.0, 2000),
            process(40, "aider", 40.0, 1000),
        ])
    }

    #[test]
    fn classifies_known_and_external_agent_processes() {
        let rows = rows(&snapshot());
        assert_eq!(
            rows.iter().map(|row| row.kind.as_str()).collect::<Vec<_>>(),
            ["Claude", "Codex-ACP", "OpenHuman", "External"]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.origin.as_str())
                .collect::<Vec<_>>(),
            ["external", "external", "external", "external"]
        );
    }

    #[test]
    fn sorts_each_column_reverses_and_filters_codex_by_regex() {
        let snap = snapshot();
        let mut state = ResourcesState::default();
        for sort in [
            AgentSort::Pid,
            AgentSort::Kind,
            AgentSort::Origin,
            AgentSort::Workspace,
            AgentSort::Threads,
            AgentSort::Memory,
            AgentSort::Cpu,
            AgentSort::Runtime,
        ] {
            state.agent_sort = sort;
            let sorted = visible_rows(&snap, &state);
            assert_eq!(sorted.len(), 4);
            assert!(sorted
                .windows(2)
                .all(|w| compare(&w[0], &w[1], sort) != Ordering::Greater));
        }
        state.agent_sort = AgentSort::Cpu;
        state.agent_reverse = true;
        assert_eq!(visible_rows(&snap, &state)[0].pid, 40);
        state.agent_reverse = false;
        state.agent_filter = Some("!^cod".into());
        assert_eq!(
            visible_rows(&snap, &state)
                .iter()
                .map(|r| r.kind.as_str())
                .collect::<Vec<_>>(),
            ["Codex-ACP"]
        );
    }
}
