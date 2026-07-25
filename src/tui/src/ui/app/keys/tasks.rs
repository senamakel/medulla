//! Keyboard handling for the Tasks tab's task-list and source-management pages.

use crossterm::event::KeyCode;

use crate::ui::composer::{insert_at, Draft};
use crate::ui::multi_pane::{self, NavAction};

use super::super::types::{App, Cmd, Prompt, PromptKind, TASKS_SUBPAGES, TP_SOURCES, TP_TASKS};

impl App {
    /// Handle Tasks navigation and actions in the currently focused page.
    pub(super) fn on_tasks_key(&mut self, code: KeyCode) -> TasksKey {
        match multi_pane::navigate(
            code,
            TASKS_SUBPAGES.len(),
            &mut self.tasks_index,
            &mut self.tasks_focused,
            true,
        ) {
            NavAction::SelectionChanged | NavAction::Consumed => {
                return TasksKey::Handled(None);
            }
            NavAction::Entered => {
                self.set_status(format!(
                    "{} · Esc to go back to the menu",
                    self.tasks_subpage()
                ));
                return TasksKey::Handled(None);
            }
            NavAction::Left => {
                self.set_status("Tasks · menu");
                return TasksKey::Handled(None);
            }
            NavAction::Unhandled => {}
        }

        match self.tasks_index {
            TP_TASKS => self.task_list_key(code),
            TP_SOURCES => self.task_sources_key(code),
            _ => TasksKey::Unhandled,
        }
    }

    /// Browse and mutate durable local tasks.
    fn task_list_key(&mut self, code: KeyCode) -> TasksKey {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                TasksKey::Handled(None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.tasks.tasks.len().saturating_sub(1));
                TasksKey::Handled(None)
            }
            KeyCode::Char('a') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::TaskCreate,
                    title: "New task title".into(),
                    draft: Draft::new(),
                });
                self.set_status("Tasks · Enter save · Esc cancel");
                TasksKey::Handled(None)
            }
            KeyCode::Char('e') => {
                if let Some(task) = self.tasks.tasks.get(self.selected).cloned() {
                    self.prompt = Some(Prompt {
                        kind: PromptKind::TaskEdit(task.id),
                        title: "Edit task title".into(),
                        draft: insert_at("", 0, &task.title),
                    });
                    self.set_status("Tasks · Enter save · Esc cancel");
                }
                TasksKey::Handled(None)
            }
            KeyCode::Char('d') => TasksKey::Handled(
                self.tasks
                    .tasks
                    .get(self.selected)
                    .map(|task| Cmd::DeleteTask(task.id.clone())),
            ),
            _ => TasksKey::Unhandled,
        }
    }

    /// Browse synchronized task sources, add a repository, or sync one source.
    fn task_sources_key(&mut self, code: KeyCode) -> TasksKey {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.task_source_index = self.task_source_index.saturating_sub(1);
                TasksKey::Handled(None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.task_source_index =
                    (self.task_source_index + 1).min(self.tasks.sources.len().saturating_sub(1));
                TasksKey::Handled(None)
            }
            KeyCode::Char('a') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::SourceAdd,
                    title: "GitHub repository (owner/name)".into(),
                    draft: Draft::new(),
                });
                self.set_status("Sources · Enter save · Esc cancel");
                TasksKey::Handled(None)
            }
            KeyCode::Char('s') | KeyCode::Enter => {
                let source = self
                    .tasks
                    .sources
                    .get(self.task_source_index)
                    .filter(|source| source.enabled);
                if let Some(source) = source {
                    TasksKey::Handled(Some(Cmd::SyncTasks(source.id.clone())))
                } else {
                    self.set_status(if self.tasks.sources.is_empty() {
                        "Sources · none configured"
                    } else {
                        "Sources · selected source is disabled"
                    });
                    TasksKey::Handled(None)
                }
            }
            _ => TasksKey::Unhandled,
        }
    }
}

/// Whether Tasks consumed a key and its optional persistence command.
pub(super) enum TasksKey {
    /// Tasks handled the key.
    Handled(Option<Cmd>),
    /// A structural/global binding may handle the key.
    Unhandled,
}
