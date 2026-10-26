//! Keys for the Agents box: selection, filter, sort, follow, and signals.

use super::types;
use crate::ui::app::types::App;

impl App {
    pub(super) fn agent_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        use types::{AgentSignal, AgentSignalConfirmStage, AgentSignalConfirmation, AgentSort};
        if !self.resources.shown.agents {
            self.resources.agent_signal_armed = None;
            return false;
        }
        if let Some(armed) = self.resources.agent_signal_armed.take() {
            let confirmation = match armed.stage {
                AgentSignalConfirmStage::First => key == KeyCode::Char('y'),
                AgentSignalConfirmStage::ExternalSecond => key == KeyCode::Char('Y'),
            };
            if !confirmation {
                self.set_status("Process signal cancelled");
                return true;
            }
            let process = self
                .resource_snapshot
                .processes
                .iter()
                .find(|process| process.pid == armed.pid && process.start_time == armed.started)
                .cloned();
            let Some(process) = process else {
                self.set_status("Process exited before signal was confirmed");
                return true;
            };
            let row = crate::ui::app::render::resources::rows(&self.resource_snapshot)
                .into_iter()
                .find(|row| row.pid == armed.pid && row.started == armed.started);
            let Some(row) = row else {
                self.set_status("Process is no longer a classified local agent");
                return true;
            };
            if row.remote {
                self.set_status("Signals to remote processes are disabled");
                return true;
            }
            if !(self.resources.agent_process_owned_fn)(
                armed.pid,
                armed.started,
                process.owner_uid.as_deref(),
            ) {
                self.set_status("Cannot signal a process not owned by the current user");
                return true;
            }
            if row.external && armed.stage == AgentSignalConfirmStage::First {
                self.resources.agent_signal_armed = Some(AgentSignalConfirmation {
                    stage: AgentSignalConfirmStage::ExternalSecond,
                    ..armed
                });
                self.set_status("External process: press uppercase Y to confirm the signal");
                return true;
            }
            match (self.resources.agent_signal_fn)(
                armed.pid,
                armed.started,
                process.owner_uid.as_deref(),
                armed.signal,
            ) {
                Ok(()) => self.set_status(format!("Signal sent to PID {}", armed.pid)),
                Err(error) => self.set_status(format!("Signal failed: {error}")),
            }
            return true;
        }
        if self.resources.agent_filter_editing {
            match key {
                KeyCode::Esc => {
                    self.resources.agent_filter_editing = false;
                }
                KeyCode::Enter => {
                    self.resources.agent_filter_editing = false;
                    let remote_hosts: Vec<_> = self
                        .remote_hosts
                        .iter()
                        .map(|(id, host)| (id.clone(), host.rows.clone()))
                        .collect();
                    let rows = crate::ui::app::render::resources::visible_rows_with_remote(
                        &self.resource_snapshot,
                        &remote_hosts,
                        &self.resources,
                    );
                    self.resources.agent_selected = rows.first().map(|row| row.identity());
                }
                KeyCode::Backspace => {
                    self.resources.agent_filter.as_mut().map(String::pop);
                }
                KeyCode::Char(c) => self
                    .resources
                    .agent_filter
                    .get_or_insert_with(String::new)
                    .push(c),
                _ => {}
            }
            return true;
        }
        if self.resources.agent_detail && key == KeyCode::Esc {
            self.resources.agent_detail = false;
            return true;
        }
        let remote_hosts: Vec<_> = self
            .remote_hosts
            .iter()
            .map(|(id, host)| (id.clone(), host.rows.clone()))
            .collect();
        let rows = crate::ui::app::render::resources::visible_rows_with_remote(
            &self.resource_snapshot,
            &remote_hosts,
            &self.resources,
        );
        let selected = self
            .resources
            .agent_selected
            .as_ref()
            .and_then(|id| rows.iter().position(|row| row.identity() == *id))
            .unwrap_or(0);
        match key {
            KeyCode::Left | KeyCode::Right => {
                self.resources.agent_sort = match (self.resources.agent_sort, key == KeyCode::Right)
                {
                    (AgentSort::Pid, true) => AgentSort::Kind,
                    (AgentSort::Kind, true) => AgentSort::Origin,
                    (AgentSort::Origin, true) => AgentSort::Workspace,
                    (AgentSort::Workspace, true) => AgentSort::Threads,
                    (AgentSort::Threads, true) => AgentSort::Memory,
                    (AgentSort::Memory, true) => AgentSort::Cpu,
                    (AgentSort::Cpu, true) => AgentSort::Runtime,
                    (AgentSort::Runtime, true) => AgentSort::Pid,
                    (AgentSort::Pid, false) => AgentSort::Runtime,
                    (AgentSort::Runtime, false) => AgentSort::Cpu,
                    (AgentSort::Cpu, false) => AgentSort::Memory,
                    (AgentSort::Memory, false) => AgentSort::Threads,
                    (AgentSort::Threads, false) => AgentSort::Workspace,
                    (AgentSort::Workspace, false) => AgentSort::Origin,
                    (AgentSort::Origin, false) => AgentSort::Kind,
                    (AgentSort::Kind, false) => AgentSort::Pid,
                };
                true
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => {
                let next = match key {
                    KeyCode::Up => selected.saturating_sub(1),
                    KeyCode::Down => (selected + 1).min(rows.len().saturating_sub(1)),
                    KeyCode::PageUp => selected.saturating_sub(10),
                    KeyCode::PageDown => (selected + 10).min(rows.len().saturating_sub(1)),
                    KeyCode::Home => 0,
                    KeyCode::End => rows.len().saturating_sub(1),
                    _ => selected,
                };
                self.resources.agent_selected = rows.get(next).map(|row| row.identity());
                if self.resources.agent_follow.is_some() {
                    self.resources.agent_follow = self.resources.agent_selected.clone();
                }
                true
            }
            KeyCode::Char('r') => {
                self.resources.agent_reverse = !self.resources.agent_reverse;
                true
            }
            KeyCode::Char('e') => {
                self.resources.agent_tree = !self.resources.agent_tree;
                true
            }
            KeyCode::Char('f') | KeyCode::Char('/') => {
                self.resources.agent_filter_editing = true;
                self.resources.agent_filter.get_or_insert_with(String::new);
                true
            }
            KeyCode::Char('F') => {
                let identity = rows
                    .get(selected)
                    .filter(|row| !row.remote)
                    .map(|row| row.identity());
                if identity.is_none() {
                    self.set_status("Follow is available for local agents only");
                    return true;
                }
                self.resources.agent_follow = match (self.resources.agent_follow.clone(), identity)
                {
                    (Some(following), Some(selected)) if following == selected => None,
                    (_, selected) => selected,
                };
                if self.resources.agent_follow.is_some() {
                    self.resources.agent_selected = self.resources.agent_follow.clone();
                }
                true
            }
            KeyCode::Enter => {
                if let Some(row) = rows.get(selected) {
                    if let Some(session) = row.session_id.as_deref() {
                        self.tab_index = crate::ui::app::types::TABS
                            .iter()
                            .position(|tab| *tab == "Sessions")
                            .unwrap_or(0);
                        self.select_session_row(session);
                    } else {
                        // `agent_selected` is `None` until a cursor key moves it,
                        // though the handler already resolved it to a row; store
                        // that row so the detail pane can find what it opens.
                        self.resources.agent_selected = Some(row.identity());
                        self.resources.agent_detail = true;
                    }
                }
                true
            }
            KeyCode::Char('t' | 'k' | 's') => {
                if let Some(row) = rows.get(selected) {
                    if !cfg!(any(target_os = "linux", target_os = "macos")) {
                        self.set_status("Process signals are disabled on this platform");
                        return true;
                    }
                    if row.remote {
                        self.set_status("Signals to remote processes are disabled");
                        return true;
                    }
                    let signal = match key {
                        KeyCode::Char('t') => AgentSignal::Terminate,
                        KeyCode::Char('k') => AgentSignal::Kill,
                        _ => AgentSignal::Stop,
                    };
                    self.resources.agent_signal_armed = Some(AgentSignalConfirmation {
                        pid: row.pid,
                        started: row.started,
                        signal,
                        stage: AgentSignalConfirmStage::First,
                    });
                }
                true
            }
            _ => false,
        }
    }
}
