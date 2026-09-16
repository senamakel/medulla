//! Overlay rendering for [`App`]: the inline prompt overlay.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use super::super::types::App;

impl App {
    /// Draw the inline prompt overlay (Workers add/edit, task answer).
    pub(super) fn draw_prompt(&mut self, f: &mut Frame, area: Rect) {
        let Some(prompt) = &self.prompt else { return };
        let block = crate::ui::widgets::panel(&self.theme, prompt.title.clone(), true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(crate::ui::widgets::prompt_line(&prompt.draft, &self.theme)),
            inner,
        );
    }

}
