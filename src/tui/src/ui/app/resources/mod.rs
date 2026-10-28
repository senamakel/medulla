//! The host Resources tab: btop, embedded, focused on coding agents.
//!
//! The tab is a window onto [`crate::ui::btop`]: drawing shows and drains the
//! shared btop pane, keys and mouse events inside the tab go to btop, and
//! btop's own quit (`q`, or Quit in its menu) returns to the Sessions tab.
//! btop is paused whenever another tab is on screen.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::{layout::Rect, Frame};

use super::types::{tab_pos, App};

#[cfg(test)]
mod tests;

impl App {
    /// Draw btop into `area`, starting it the first time the tab is shown.
    pub(super) fn draw_resources(&mut self, f: &mut Frame, area: Rect) {
        self.resources_area = area;
        let config_dir = self.btop_config_dir();
        let mut pane = crate::ui::btop::pane();
        pane.show(&config_dir, area);
        let leave = pane.pump();
        pane.render(f.buffer_mut(), area);
        drop(pane);
        if leave {
            self.tab_index = tab_pos("Sessions");
            self.selected = 0;
        }
    }

    /// Pause btop when the Resources tab is not the one being drawn.
    pub(super) fn hide_resources_unless_shown(&self) {
        if self.tab() != "Resources" {
            crate::ui::btop::pane().hide();
        }
    }

    /// Route `k` to btop. Everything goes to it except tab switching (Tab,
    /// Shift-Tab) and the global chords (⌃C quit, ⌃O mouse, ⌃T new session),
    /// which stay Medulla's on every tab. Returns whether btop took the key.
    pub(super) fn resources_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let global = matches!(k.code, KeyCode::Tab | KeyCode::BackTab)
            || (ctrl && matches!(k.code, KeyCode::Char('c' | 'o' | 't')));
        !global && crate::ui::btop::pane().key(k)
    }

    /// Route a mouse event inside the tab's content to btop.
    pub(super) fn resources_mouse(&mut self, m: &MouseEvent) -> bool {
        crate::ui::btop::pane().mouse(*m, self.resources_area)
    }

    /// Where btop keeps `btop.conf` and its log: beside Medulla's own state
    /// rather than in the operator's `~/.config/btop`, so Medulla's defaults
    /// (the agents box, tree view) never rewrite a standalone btop's config.
    /// An app with no home (a test) gets a per-process scratch directory.
    fn btop_config_dir(&self) -> PathBuf {
        match &self.medulla_home {
            Some(home) => home.join("btop"),
            None => std::env::temp_dir().join(format!("medulla-btop-{}", std::process::id())),
        }
    }
}
