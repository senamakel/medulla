//! Keyboard handling for the TokenMaxxing overview and bounty pages.

use crossterm::event::KeyCode;

use crate::ui::multi_pane::{self, NavAction};

use super::super::types::{App, Cmd, TOKENMAXXING_SUBPAGES};

impl App {
    /// Move between the TokenMaxxing menu and its read-only content pages.
    pub(super) fn on_tokenmaxxing_key(&mut self, code: KeyCode) -> TokenMaxxingKey {
        match multi_pane::navigate(
            code,
            TOKENMAXXING_SUBPAGES.len(),
            &mut self.tokenmaxxing_index,
            &mut self.tokenmaxxing_focused,
            true,
        ) {
            NavAction::SelectionChanged | NavAction::Consumed => TokenMaxxingKey::Handled(None),
            NavAction::Entered => {
                self.set_status(format!(
                    "TokenMaxxing · {} · Esc to go back to the menu",
                    TOKENMAXXING_SUBPAGES[self.tokenmaxxing_index]
                ));
                TokenMaxxingKey::Handled(None)
            }
            NavAction::Left => {
                self.set_status("TokenMaxxing · menu");
                TokenMaxxingKey::Handled(None)
            }
            NavAction::Unhandled => TokenMaxxingKey::Unhandled,
        }
    }
}

mod types;
pub(super) use types::TokenMaxxingKey;
