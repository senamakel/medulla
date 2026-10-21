use super::types::ResourcesState;
use crossterm::event::KeyCode;

impl ResourcesState {
    pub(in crate::ui::app) fn key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('1') => self.shown.cpu = !self.shown.cpu,
            KeyCode::Char('2') => self.shown.mem = !self.shown.mem,
            KeyCode::Char('3') => self.shown.net = !self.shown.net,
            KeyCode::Char('4') => self.shown.agents = !self.shown.agents,
            KeyCode::Char('p') => self.preset = self.preset.saturating_sub(1),
            KeyCode::Char('P') => self.preset = (self.preset + 1) % 10,
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.interval_ms = (self.interval_ms + 100).min(10_000)
            }
            KeyCode::Char('-') => self.interval_ms = self.interval_ms.saturating_sub(100).max(100),
            _ => return false,
        }
        true
    }
}
