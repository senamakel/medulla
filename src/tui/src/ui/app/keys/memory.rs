//! Keyboard handling for Memory overview, search, and maintenance pages.

use crossterm::event::KeyCode;

use crate::ui::composer::Draft;
use crate::ui::multi_pane::{self, NavAction};

use super::super::types::{
    App, Cmd, Prompt, PromptKind, MEMORY_SUBPAGES, MP_MAINTENANCE, MP_OVERVIEW, MP_SEARCH,
};

impl App {
    /// Handle Memory navigation and the active page's actions.
    pub(super) fn on_memory_key(&mut self, code: KeyCode) -> MemoryKey {
        match multi_pane::navigate(
            code,
            MEMORY_SUBPAGES.len(),
            &mut self.memory_subpage_index,
            &mut self.memory_focused,
            true,
        ) {
            NavAction::SelectionChanged | NavAction::Consumed => {
                return MemoryKey::Handled(None);
            }
            NavAction::Entered => {
                self.set_status(format!(
                    "{} · Esc to go back to the menu",
                    self.memory_subpage()
                ));
                return MemoryKey::Handled(None);
            }
            NavAction::Left => {
                self.set_status("Memory · menu");
                return MemoryKey::Handled(None);
            }
            NavAction::Unhandled => {}
        }

        match self.memory_subpage_index {
            MP_OVERVIEW => self.memory_overview_key(code),
            MP_SEARCH => self.memory_search_key(code),
            MP_MAINTENANCE => self.memory_maintenance_key(code),
            _ => MemoryKey::Unhandled,
        }
    }

    /// Browse persona directives and facet summaries.
    fn memory_overview_key(&mut self, code: KeyCode) -> MemoryKey {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.memory_index = self.memory_index.saturating_sub(1);
                MemoryKey::Handled(None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.memory_overview_entry_count().saturating_sub(1);
                self.memory_index = (self.memory_index + 1).min(max);
                MemoryKey::Handled(None)
            }
            KeyCode::Char('r') => self.reload_memory(),
            _ => MemoryKey::Unhandled,
        }
    }

    /// Browse ranked results or open the query prompt.
    fn memory_search_key(&mut self, code: KeyCode) -> MemoryKey {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.memory_index = self.memory_index.saturating_sub(1);
                MemoryKey::Handled(None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.memory_hits.len().saturating_sub(1);
                self.memory_index = (self.memory_index + 1).min(max);
                MemoryKey::Handled(None)
            }
            KeyCode::Enter | KeyCode::Char('/') | KeyCode::Char('q') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::MemorySearch,
                    title: "Search persona memory".into(),
                    draft: Draft::new(),
                });
                self.set_status("Memory search · Enter run · Esc cancel");
                MemoryKey::Handled(None)
            }
            _ => MemoryKey::Unhandled,
        }
    }

    /// Refresh or ingest persona memory.
    fn memory_maintenance_key(&mut self, code: KeyCode) -> MemoryKey {
        match code {
            KeyCode::Char('r') => self.reload_memory(),
            KeyCode::Char('b') | KeyCode::Char('i') => {
                let backfill = matches!(code, KeyCode::Char('b'));
                MemoryKey::Handled(self.start_memory_ingest(backfill))
            }
            _ => MemoryKey::Unhandled,
        }
    }

    /// Ask the event loop to refresh the memory status and persona directives.
    fn reload_memory(&mut self) -> MemoryKey {
        self.set_status("Memory · refreshing…");
        MemoryKey::Handled(Some(Cmd::LoadMemory))
    }
}

/// Whether Memory consumed a key and its optional service command.
pub(super) enum MemoryKey {
    /// Memory handled the key.
    Handled(Option<Cmd>),
    /// A structural/global binding may handle the key.
    Unhandled,
}
