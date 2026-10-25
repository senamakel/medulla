//! btop-style box chrome shared by the Resources panels.
//!
//! A panel is a rounded frame whose top and bottom borders carry bracketed
//! tags, `─┐¹cpu┌┐menu┌──`. A tag's hotkey letter is drawn in the theme's
//! `hi_fg` so the border reads as the box's key legend, the way btop's does.

use crate::ui::theme::{Gradient, ResourcesTheme};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Widget},
};

const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

/// Paint `bg` behind every cell of `area`, keeping what is drawn there.
pub(in crate::ui::app) fn fill(buffer: &mut Buffer, area: Rect, bg: Color) {
    buffer.set_style(area.intersection(buffer.area), Style::default().bg(bg));
}

/// Draw a rounded frame in `line` and return its inner area.
pub(in crate::ui::app) fn frame(buffer: &mut Buffer, area: Rect, line: Color) -> Rect {
    if area.width < 2 || area.height < 2 {
        return Rect::default();
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(line));
    let inner = block.inner(area);
    block.render(area, buffer);
    inner
}

/// Tags packed onto one border row, left tags growing rightwards and right
/// tags growing leftwards until they would meet.
pub(in crate::ui::app) struct Bar {
    y: u16,
    left: u16,
    right: u16,
    line: Color,
    bottom: bool,
}

impl Bar {
    /// Tags on the top border of `area`, a frame drawn by [`frame`].
    pub(in crate::ui::app) fn top(area: Rect, line: Color) -> Self {
        Self::new(area, area.y, line, false)
    }

    /// Tags on the bottom border of `area`.
    pub(in crate::ui::app) fn bottom(area: Rect, line: Color) -> Self {
        Self::new(area, area.bottom().saturating_sub(1), line, true)
    }

    fn new(area: Rect, y: u16, line: Color, bottom: bool) -> Self {
        Self {
            y,
            left: area.x.saturating_add(2),
            right: area.right().saturating_sub(2),
            line,
            bottom,
        }
    }

    /// Place a tag after the previous left tag. Returns the rectangle of its
    /// text, or an empty one when it does not fit.
    pub(in crate::ui::app) fn left(&mut self, buffer: &mut Buffer, spans: &[Span]) -> Rect {
        let width = spans_width(spans) + 2;
        if self.left + width > self.right {
            return Rect::default();
        }
        let text = self.put(buffer, self.left, spans);
        self.left += width;
        text
    }

    /// Place a tag before the previous right tag.
    pub(in crate::ui::app) fn right(&mut self, buffer: &mut Buffer, spans: &[Span]) -> Rect {
        let width = spans_width(spans) + 2;
        if self.right < self.left + width {
            return Rect::default();
        }
        self.right -= width;
        self.put(buffer, self.right, spans)
    }

    /// Centre a tag between the current left and right tags. Call it last:
    /// it does not reserve space for later tags.
    pub(in crate::ui::app) fn center(&mut self, buffer: &mut Buffer, spans: &[Span]) -> Rect {
        let width = spans_width(spans) + 2;
        let room = self.right.saturating_sub(self.left);
        if room < width + 2 {
            return Rect::default();
        }
        self.put(buffer, self.left + (room - width) / 2, spans)
    }

    fn put(&self, buffer: &mut Buffer, x: u16, spans: &[Span]) -> Rect {
        let (open, close) = if self.bottom {
            ("┘", "└")
        } else {
            ("┐", "┌")
        };
        let line = Style::default().fg(self.line);
        buffer.set_string(x, self.y, open, line);
        let mut cursor = x + 1;
        for span in spans {
            let (next, _) = buffer.set_span(cursor, self.y, span, span.width() as u16);
            cursor = next;
        }
        buffer.set_string(cursor, self.y, close, line);
        Rect::new(x + 1, self.y, cursor - x - 1, 1)
    }
}

/// `¹cpu`: the box number as a hotkey superscript, then its name.
pub(in crate::ui::app) fn numbered(
    theme: &ResourcesTheme,
    number: usize,
    name: &str,
) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            SUPERSCRIPTS[number % 10].to_string(),
            Style::default().fg(theme.hi_fg),
        ),
        Span::styled(
            name.to_owned(),
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ),
    ]
}

/// A button label with its first `key` character highlighted. An inactive
/// toggle is drawn dim, as btop draws a switched-off option.
pub(in crate::ui::app) fn keyed(
    theme: &ResourcesTheme,
    text: &str,
    key: char,
    active: bool,
) -> Vec<Span<'static>> {
    let base = Style::default().fg(if active {
        theme.title
    } else {
        theme.inactive_fg
    });
    let Some(index) = text.find(key) else {
        return vec![Span::styled(text.to_owned(), base)];
    };
    let end = index + key.len_utf8();
    vec![
        Span::styled(text[..index].to_owned(), base),
        Span::styled(
            text[index..end].to_owned(),
            Style::default()
                .fg(theme.hi_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(text[end..].to_owned(), base),
    ]
}

/// `‹ text ›` or `- text +`: a value flanked by two one-cell buttons.
pub(in crate::ui::app) fn stepper(
    theme: &ResourcesTheme,
    down: &str,
    text: &str,
    up: &str,
) -> Vec<Span<'static>> {
    let key = Style::default()
        .fg(theme.hi_fg)
        .add_modifier(Modifier::BOLD);
    vec![
        Span::styled(down.to_owned(), key),
        Span::styled(format!(" {text} "), Style::default().fg(theme.title)),
        Span::styled(up.to_owned(), key),
    ]
}

/// Plain title-coloured tag text.
pub(in crate::ui::app) fn plain(theme: &ResourcesTheme, text: &str) -> Vec<Span<'static>> {
    vec![Span::styled(
        text.to_owned(),
        Style::default().fg(theme.title),
    )]
}

/// The down and up buttons of a [`stepper`] tag, `down` and `up` cells wide
/// at either end of its text rectangle.
pub(in crate::ui::app) fn stepper_hits(text: Rect, down: u16, up: u16) -> (Rect, Rect) {
    if text.width < down + up {
        return (Rect::default(), Rect::default());
    }
    (
        Rect::new(text.x, text.y, down, 1),
        Rect::new(text.right() - up, text.y, up, 1),
    )
}

/// Draw `text` in `style`, clipped to `width` cells.
pub(in crate::ui::app) fn text(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    text: &str,
    style: Style,
) -> u16 {
    if width == 0 {
        return x;
    }
    buffer.set_stringn(x, y, text, width as usize, style).0
}

/// Draw `text` right-aligned so it ends at `right`.
pub(in crate::ui::app) fn text_right(
    buffer: &mut Buffer,
    right: u16,
    y: u16,
    min_x: u16,
    value: &str,
    style: Style,
) {
    let width = value.chars().count() as u16;
    let x = right.saturating_sub(width).max(min_x);
    text(buffer, x, y, right.saturating_sub(x), value, style);
}

/// Sample a precomputed theme gradient into a plot gradient.
pub(in crate::ui::app) fn plot_gradient(gradient: Gradient) -> crate::ui::plot::Gradient {
    crate::ui::plot::Gradient::new((0..=100).map(|pct| gradient.at(pct)).collect::<Vec<_>>())
}

/// A colour from `gradient` for a percentage.
pub(in crate::ui::app) fn shade(gradient: &Gradient, percent: f64) -> Color {
    gradient.at(if percent.is_finite() {
        percent.clamp(0.0, 100.0).round() as usize
    } else {
        0
    })
}

/// Binary byte size in btop's style: `17.7 GiB`, `950 MiB`, `12 KiB`.
pub(in crate::ui::app) fn bytes(value: f64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = value.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// A short binary size for tight columns: `17.7G`, `950M`, `12K`.
pub(in crate::ui::app) fn bytes_short(value: f64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut value = value.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 10.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Bit rate in btop's style: `470 Kibps`, `2.46 Mibps`.
///
/// At most 10 cells, so a rate line in the 25-cell stats box always fits.
pub(in crate::ui::app) fn bits(bytes_per_second: f64) -> String {
    const UNITS: [&str; 5] = ["bps", "Kibps", "Mibps", "Gibps", "Tibps"];
    let mut value = (bytes_per_second * 8.0).max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn spans_width(spans: &[Span]) -> u16 {
    spans.iter().map(|span| span.width() as u16).sum()
}
