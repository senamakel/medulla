//! Blit btop's emulated screen into the ratatui frame, cell for cell.
//!
//! btop positions everything absolutely and repaints only what changed, so the
//! emulator holds the authoritative picture and the frame simply copies it.
//! Copying cells (rather than building styled lines) keeps every box border and
//! braille graph glyph exactly where btop put it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::worker::screen::vt_color;

/// Copy the top-left `area`-sized region of `screen` into `buf` at `area`.
pub fn blit(screen: &vt100::Screen, buf: &mut Buffer, area: Rect) {
    let area = area.intersection(buf.area);
    for row in 0..area.height {
        for col in 0..area.width {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            let target = &mut buf[(area.x + col, area.y + row)];
            if cell.is_wide_continuation() {
                // The wide glyph to the left already covers this column.
                target.set_symbol("");
                continue;
            }
            let contents = cell.contents();
            target.set_symbol(if contents.is_empty() { " " } else { &contents });
            target.set_style(cell_style(cell));
        }
    }
}

/// The ratatui style for one emulator cell. Inverse swaps the colours, for
/// the reason given at [`crate::worker::screen::cell_style`].
pub fn cell_style(cell: &vt100::Cell) -> Style {
    let (fg, bg) = if cell.inverse() {
        (cell.bgcolor(), cell.fgcolor())
    } else {
        (cell.fgcolor(), cell.bgcolor())
    };
    let mut style = Style::default().fg(vt_color(fg)).bg(vt_color(bg));
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}
