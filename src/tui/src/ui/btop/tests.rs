//! The pure halves of the btop pane: event encoding, the screen blit, and the
//! agent table built from classified processes.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use medulla::resources::agents::{AgentKind, AgentOrigin, AgentProcess, ProcessUsage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use super::feed::tags;
use super::input::{key_bytes, mouse_bytes};
use super::render::blit;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn keys_encode_as_the_bytes_an_xterm_sends() {
    let none = KeyModifiers::NONE;
    assert_eq!(key_bytes(key(KeyCode::Char('A'), none)), Some(b"A".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Char('é'), none)), Some("é".as_bytes().to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Enter, none)), Some(b"\n".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Esc, none)), Some(b"\x1b".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Up, none)), Some(b"\x1b[A".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Left, none)), Some(b"\x1b[D".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::PageDown, none)), Some(b"\x1b[6~".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::Delete, none)), Some(b"\x1b[3~".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::BackTab, none)), Some(b"\x1b[Z".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::F(1), none)), Some(b"\x1bOP".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::F(12), none)), Some(b"\x1b[24~".to_vec()));
    assert_eq!(key_bytes(key(KeyCode::F(13), none)), None);
    assert_eq!(key_bytes(key(KeyCode::CapsLock, none)), None);
}

#[test]
fn control_and_alt_modify_the_encoding() {
    // ⌃R is btop's config reload.
    assert_eq!(
        key_bytes(key(KeyCode::Char('r'), KeyModifiers::CONTROL)),
        Some(vec![0x12])
    );
    assert_eq!(
        key_bytes(key(KeyCode::Char('R'), KeyModifiers::CONTROL)),
        Some(vec![0x12])
    );
    assert_eq!(key_bytes(key(KeyCode::Char('1'), KeyModifiers::CONTROL)), None);
    assert_eq!(
        key_bytes(key(KeyCode::Char('x'), KeyModifiers::ALT)),
        Some(b"\x1bx".to_vec())
    );
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_events_become_sgr_reports_relative_to_btops_screen() {
    let area = Rect::new(10, 2, 80, 20);
    let down = MouseEventKind::Down(MouseButton::Left);
    // The pane's top-left cell is btop's (1, 1).
    assert_eq!(
        mouse_bytes(mouse(down, 10, 2), area),
        Some(b"\x1b[<0;1;1M".to_vec())
    );
    assert_eq!(
        mouse_bytes(mouse(MouseEventKind::Up(MouseButton::Left), 14, 7), area),
        Some(b"\x1b[<0;5;6m".to_vec())
    );
    assert_eq!(
        mouse_bytes(mouse(MouseEventKind::Drag(MouseButton::Left), 11, 3), area),
        Some(b"\x1b[<32;2;2M".to_vec())
    );
    assert_eq!(
        mouse_bytes(mouse(MouseEventKind::ScrollUp, 11, 3), area),
        Some(b"\x1b[<64;2;2M".to_vec())
    );
    assert_eq!(
        mouse_bytes(mouse(MouseEventKind::ScrollDown, 11, 3), area),
        Some(b"\x1b[<65;2;2M".to_vec())
    );
}

#[test]
fn mouse_events_outside_btop_or_unused_by_it_are_not_sent() {
    let area = Rect::new(10, 2, 80, 20);
    let down = MouseEventKind::Down(MouseButton::Left);
    // The tab bar above the pane, and just past its right and bottom edges.
    assert_eq!(mouse_bytes(mouse(down, 10, 1), area), None);
    assert_eq!(mouse_bytes(mouse(down, 90, 5), area), None);
    assert_eq!(mouse_bytes(mouse(down, 20, 22), area), None);
    assert_eq!(
        mouse_bytes(mouse(MouseEventKind::Down(MouseButton::Right), 20, 5), area),
        None
    );
    assert_eq!(mouse_bytes(mouse(MouseEventKind::Moved, 20, 5), area), None);
}

#[test]
fn the_blit_copies_glyphs_and_colours_cell_for_cell() {
    let mut parser = vt100::Parser::new(3, 10, 0);
    // A truecolour bold "cpu" at (1,2), an inverse cell, and a wide glyph.
    parser.process(b"\x1b[2;3H\x1b[1;38;2;1;2;3mcpu\x1b[0m\x1b[3;1H\x1b[7mX\x1b[0m\x1b[3;4H\xe2\x96\x88\xe7\x95\x8c");
    let area = Rect::new(5, 1, 10, 3);
    let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
    blit(parser.screen(), &mut buf, area);

    let c = &buf[(5 + 2, 1 + 1)];
    assert_eq!(c.symbol(), "c");
    assert_eq!(c.fg, Color::Rgb(1, 2, 3));
    assert!(c.modifier.contains(Modifier::BOLD));
    assert_eq!(buf[(5 + 4, 2)].symbol(), "u");
    // Blank emulator cells become spaces, not stale buffer content.
    assert_eq!(buf[(5, 1)].symbol(), " ");
    // Inverse swaps colours rather than relying on REVERSED.
    let inverse = &buf[(5, 3)];
    assert_eq!(inverse.symbol(), "X");
    assert!(!inverse.modifier.contains(Modifier::REVERSED));
    // The wide glyph takes two columns; the second is left to it.
    assert_eq!(buf[(5 + 3, 3)].symbol(), "█");
    assert_eq!(buf[(5 + 4, 3)].symbol(), "界");
    assert_eq!(buf[(5 + 5, 3)].symbol(), "");
    // Nothing is drawn outside the area.
    assert_eq!(buf[(4, 1)].symbol(), " ");
    assert_eq!(buf[(15, 1)].symbol(), " ");
}

#[test]
fn the_blit_clips_to_the_frame() {
    let parser = vt100::Parser::new(5, 5, 0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 3, 3));
    // An area hanging off the buffer's edge must not panic.
    blit(parser.screen(), &mut buf, Rect::new(1, 1, 5, 5));
}

fn agent(pid: Option<u32>, kind: AgentKind, origin: AgentOrigin) -> AgentProcess {
    AgentProcess {
        pid,
        kind,
        origin,
        exe: None,
        cwd: None,
        started_at_secs: 0,
        runtime_secs: 0,
        usage: ProcessUsage::default(),
        children: Vec::new(),
    }
}

#[test]
fn classified_agents_become_btops_agent_table() {
    let table = tags(
        &[
            agent(Some(41), AgentKind::Claude, AgentOrigin::External),
            agent(
                Some(42),
                AgentKind::Codex,
                AgentOrigin::Session { id: "s1".into() },
            ),
            agent(Some(43), AgentKind::Opencode, AgentOrigin::Headless),
            agent(None, AgentKind::EmbeddedOpenhuman, AgentOrigin::Headless),
        ],
        7,
    );
    let rows: Vec<_> = table
        .iter()
        .map(|t| (t.pid, t.label.as_str(), t.launched))
        .collect();
    assert_eq!(
        rows,
        vec![
            (41, "Claude", false),
            (42, "Codex", true),
            (43, "OpenCode", true),
            // In-process: tagged on Medulla's own pid.
            (7, "OpenHuman", true),
        ]
    );
}
