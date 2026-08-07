//! The pinned detail footer: what the selected row does, every value it can
//! take, and where the answer is written.
//!
//! The choice list is the reason the footer exists. A row shows one value, and a
//! value alone tells an operator nothing about what else is available or what
//! `←/→` is about to do — they would have to press it and watch. Listing the set
//! with the current member highlighted answers both without a keystroke.

use medulla::config::StatusLineConfig;
use ratatui::style::Style;
use ratatui::text::{Line as TLine, Span};
use unicode_width::UnicodeWidthStr;

use crate::ui::app::status_line::STATUS_LINE_ROWS;
use crate::ui::app::types::App;

/// Return the number of terminal rows a wrapped footer will occupy at `width`.
///
/// Ratatui wraps each logical line independently at word boundaries. An empty
/// line still occupies a row. Measuring with the same word-boundary rule before
/// splitting the page prevents wrapped help from stealing rows reserved for the
/// bottom of the footer.
pub(super) fn rendered_height(lines: &[TLine<'_>], width: u16) -> u16 {
    lines
        .iter()
        .map(|line| wrapped_line_height(line, width))
        .sum::<usize>()
        .min(usize::from(u16::MAX)) as u16
}

/// Count rows using the same whitespace-preserving word wrapping as the footer.
fn wrapped_line_height(line: &TLine<'_>, width: u16) -> usize {
    let width = width.max(1);
    let mut rows = 1;
    let mut row_width = 0;
    let mut whitespace_width = 0;
    let mut word_width = 0;

    for grapheme in line.styled_graphemes(Style::default()) {
        let grapheme_width = grapheme.symbol.width() as u16;
        if grapheme_width > width {
            continue;
        }
        if grapheme.symbol.chars().all(char::is_whitespace) {
            if word_width > 0 {
                if row_width > 0 && row_width + whitespace_width + word_width > width {
                    rows += 1;
                    row_width = 0;
                    whitespace_width = 0;
                }
                row_width += whitespace_width + word_width;
                whitespace_width = 0;
                word_width = 0;
            }
            whitespace_width += grapheme_width;
        } else {
            word_width += grapheme_width;
        }
    }

    if word_width > 0 && row_width > 0 && row_width + whitespace_width + word_width > width {
        rows += 1;
    }
    rows
}

impl App {
    /// Build the footer lines for the selected row, most specific first.
    ///
    /// Ordered so that truncating from the bottom on a short pane drops the
    /// least specific lines: the explanation and the choices outlive the key
    /// hints, which outlive the file the change lands in.
    pub(super) fn status_line_footer(
        &self,
        selected: usize,
        cfg: &StatusLineConfig,
        dim: Style,
        width: usize,
    ) -> Vec<TLine<'static>> {
        let row = STATUS_LINE_ROWS[selected.min(STATUS_LINE_ROWS.len() - 1)];
        let (value, _) = row.field.value(cfg);

        let mut lines = vec![
            TLine::from(Span::styled("─".repeat(width), dim)),
            TLine::from(Span::styled(row.help, dim)),
        ];

        let mut spans: Vec<Span<'static>> = Vec::new();
        for (index, choice) in row.field.choices().into_iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(" · ", dim));
            }
            let style = if choice == value {
                self.theme.selection()
            } else {
                dim
            };
            spans.push(Span::styled(choice, style));
        }
        lines.push(TLine::from(spans));

        lines.push(TLine::from(Span::styled(
            format!("statusLine.{} · j/k select · ←/→ change", row.field.key()),
            dim,
        )));
        // Config is layered, so a higher-precedence file can still override what
        // was just written; an operator whose change did not stick needs to know
        // which file was tried. Clipped from the front rather than wrapped: a
        // wrapped path would push itself off a footer sized in whole lines, and
        // the tail is the half that identifies the file anyway.
        lines.push(TLine::from(Span::styled(
            match &self.config_path {
                Some(path) => {
                    let text = format!("saved to {}", path.display());
                    medulla::ui::util::clip_left(&text, width)
                }
                None => "changes apply live (no config path set)".into(),
            },
            dim,
        )));

        lines
    }
}
