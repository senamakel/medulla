//! Rendering for the Tasks tab's task-list and source-management pages.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::super::types::{TASKS_SUBPAGES, TP_SOURCES, TP_TASKS};
use super::super::App;
use crate::ui::multi_pane;
use crate::ui::util::clip;

impl App {
    /// Render the Tasks menu and its active content page.
    pub(super) fn draw_tasks(&mut self, frame: &mut Frame, area: Rect) {
        let (nav, content) = multi_pane::split(area);
        multi_pane::draw_nav(
            frame,
            nav,
            self.panel("Tasks"),
            &self.theme,
            &TASKS_SUBPAGES,
            &[],
            self.tasks_index,
            self.tasks_focused,
        );
        match self.tasks_index {
            TP_TASKS => self.draw_task_list(frame, content),
            TP_SOURCES => self.draw_task_sources(frame, content),
            _ => self.draw_task_list(frame, content),
        }
    }

    /// Render local tasks in a selectable list with a description detail pane.
    fn draw_task_list(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        let mut list = Vec::new();
        if self.tasks.tasks.is_empty() {
            list.push(Line::from(Span::styled(
                "No tasks yet · add tasks in the local repository",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        for (index, task) in self.tasks.tasks.iter().enumerate() {
            let selected = index == self.selected.min(self.tasks.tasks.len().saturating_sub(1));
            let style = if selected {
                self.theme.selection()
            } else {
                Style::default()
            };
            list.push(Line::from(vec![
                Span::styled(if selected { "▸ " } else { "  " }, style),
                Span::styled(clip(&task.title, 28), style),
                Span::styled(
                    format!("  [{:?}]", task.status),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ]));
        }
        frame.render_widget(
            Paragraph::new(list).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("All Tasks · a add · e edit · d delete"),
            ),
            columns[0],
        );
        let detail = self.tasks.tasks.get(self.selected.min(self.tasks.tasks.len().saturating_sub(1)))
            .map(|task| format!("{}\n\n{}\n\nstatus: {:?}\nsource: {}\ncreated: {}\nupdated: {}\nlast sync: {}", task.title, task.description, task.status, task.source.as_ref().map(|s| format!("{}:{}", s.provider, s.source_id)).unwrap_or_else(|| "local".into()), task.created_at, task.updated_at, task.last_synced_at.as_deref().unwrap_or("never")))
            .unwrap_or_else(|| "Select a task to view its details.\n\nSources are configured in tasks.json under the Medulla home.".into());
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title("Details")),
            columns[1],
        );
    }

    /// Render configured external task providers and their synchronization state.
    fn draw_task_sources(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        let mut rows = Vec::new();
        for (index, source) in self.tasks.sources.iter().enumerate() {
            let selected = index
                == self
                    .task_source_index
                    .min(self.tasks.sources.len().saturating_sub(1));
            let style = if selected {
                self.theme.selection()
            } else {
                Style::default()
            };
            let state = if source.enabled {
                "enabled"
            } else {
                "disabled"
            };
            rows.push(Line::from(Span::styled(
                format!(
                    "{}{} · {}",
                    if selected { "▸ " } else { "  " },
                    clip(&source.repository, 30),
                    state
                ),
                style,
            )));
        }
        if rows.is_empty() {
            rows.push(Line::from(Span::styled(
                "No sources configured · press a to add GitHub",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        frame.render_widget(
            Paragraph::new(rows).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Sources · a add · Enter/s sync"),
            ),
            columns[0],
        );

        let detail = self
            .tasks
            .sources
            .get(
                self.task_source_index
                    .min(self.tasks.sources.len().saturating_sub(1)),
            )
            .map(|source| {
                format!(
                    "{}\n\nprovider: {}\nstate filter: {}\nlabels: {}\ncustom filter: {}\ncredentials: {}",
                    source.repository,
                    source.provider,
                    source.state,
                    if source.labels.is_empty() {
                        "(none)".into()
                    } else {
                        source.labels.join(", ")
                    },
                    source.filter.as_deref().unwrap_or("(none)"),
                    if source.token.is_some() {
                        "configured"
                    } else {
                        "GITHUB_TOKEN / default"
                    },
                )
            })
            .unwrap_or_else(|| {
                "Add a GitHub repository source to synchronize open issues into the local task list."
                    .into()
            });
        frame.render_widget(
            Paragraph::new(detail).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Source Details"),
            ),
            columns[1],
        );
    }
}
