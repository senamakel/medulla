//! Encode crossterm events as the bytes a terminal would send btop.
//!
//! btop parses raw terminal input itself (`Input::get` in btop_input.cpp), so
//! the host does not translate keys into btop actions; it reproduces what an
//! xterm would have written. Each event becomes one chunk, which btop reads as
//! one key — tidier than a real terminal, where a burst can arrive glued.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// The bytes for `key`, or `None` for a key btop has no use for.
pub fn key_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let mut bytes = match key.code {
        KeyCode::Char(c) if ctrl => {
            let lower = c.to_ascii_lowercase();
            if !lower.is_ascii_lowercase() {
                return None;
            }
            vec![(lower as u8) & 0x1f]
        }
        KeyCode::Char(c) => c.to_string().into_bytes(),
        // btop keeps the terminal's ICRNL, so Enter reaches it as a newline.
        KeyCode::Enter => b"\n".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::F(n) => function_key(n)?.as_bytes().to_vec(),
        _ => return None,
    };
    if alt {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

/// xterm's encoding of F1–F12.
fn function_key(n: u8) -> Option<&'static str> {
    Some(match n {
        1 => "\x1bOP",
        2 => "\x1bOQ",
        3 => "\x1bOR",
        4 => "\x1bOS",
        5 => "\x1b[15~",
        6 => "\x1b[17~",
        7 => "\x1b[18~",
        8 => "\x1b[19~",
        9 => "\x1b[20~",
        10 => "\x1b[21~",
        11 => "\x1b[23~",
        12 => "\x1b[24~",
        _ => return None,
    })
}

/// The SGR mouse report for `event`, in coordinates relative to `area` (btop's
/// whole screen), or `None` when the event is outside it or btop ignores it.
pub fn mouse_bytes(event: MouseEvent, area: Rect) -> Option<Vec<u8>> {
    let inside = event.column >= area.x
        && event.column < area.x.saturating_add(area.width)
        && event.row >= area.y
        && event.row < area.y.saturating_add(area.height);
    if !inside {
        return None;
    }
    let (button, release) = match event.kind {
        MouseEventKind::Down(MouseButton::Left) => (0, false),
        MouseEventKind::Up(MouseButton::Left) => (0, true),
        MouseEventKind::Drag(MouseButton::Left) => (32, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        _ => return None,
    };
    let col = event.column - area.x + 1;
    let row = event.row - area.y + 1;
    let end = if release { 'm' } else { 'M' };
    Some(format!("\x1b[<{button};{col};{row}{end}").into_bytes())
}
