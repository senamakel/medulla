//! The emulator-to-wire conversion, and the round trip back.
//!
//! Pure: every test here builds a snapshot or a grid by hand and asserts on the
//! other side of [`convert`](crate::worker::stream::convert), with no pty and no
//! runtime.

use medulla::protocol::{Color, ATTR_BOLD, ATTR_INVERSE, ATTR_UNDERLINE};

use crate::worker::pty::{CellText, ScreenCell, ScreenSnapshot};
use crate::worker::stream::convert::{snapshot_from_grid, wire_grid};

/// A cell with text and no styling.
fn cell(text: &str) -> ScreenCell {
    ScreenCell {
        text: text.into(),
        ..ScreenCell::default()
    }
}

/// A snapshot from rows of text, one cell per character.
fn snapshot(rows: &[&str]) -> ScreenSnapshot {
    ScreenSnapshot {
        cells: rows
            .iter()
            .map(|row| row.chars().map(|c| cell(&c.to_string())).collect())
            .collect(),
        cursor: (0, 0),
        hide_cursor: false,
    }
}

// --- conversion ------------------------------------------------------------

#[test]
fn colours_map_across_without_being_resolved() {
    // Default must stay Default: the viewer inherits its own palette for
    // unstyled text, exactly as the local renderer does.
    assert_eq!(wire_color(vt100::Color::Default), Color::Default);
    assert_eq!(wire_color(vt100::Color::Idx(4)), Color::Idx(4));
    assert_eq!(wire_color(vt100::Color::Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
}

#[test]
fn attributes_become_flags() {
    let styled = ScreenCell {
        text: "x".into(),
        bold: true,
        underline: true,
        ..ScreenCell::default()
    };
    let style = wire_style(&styled);
    assert!(style.has(ATTR_BOLD));
    assert!(style.has(ATTR_UNDERLINE));
    assert!(!style.has(ATTR_INVERSE));
}

#[test]
fn inverse_is_carried_as_a_flag_not_baked_into_the_colours() {
    // The local renderer swaps fg/bg at paint time because terminals disagree
    // about REVERSED. Baking that into the wire would leave the viewer unable to
    // tell an inverted cell from a deliberately colour-swapped one.
    let inverted = ScreenCell {
        text: "x".into(),
        fg: vt100::Color::Idx(1),
        bg: vt100::Color::Idx(7),
        inverse: true,
        ..ScreenCell::default()
    };
    let style = wire_style(&inverted);
    assert_eq!(style.fg, Color::Idx(1), "colours must not be pre-swapped");
    assert_eq!(style.bg, Color::Idx(7));
    assert!(style.has(ATTR_INVERSE));
}

#[test]
fn a_grid_takes_its_size_from_the_snapshot() {
    let grid = wire_grid(&snapshot(&["abcd", "efgh", "ijkl"]));
    assert_eq!(grid.rows, 3);
    assert_eq!(grid.cols, 4);
    assert_eq!(grid.lines.len(), 3);
    assert_eq!(grid.lines[0], vec![ScreenRun::plain("abcd")]);
}

#[test]
fn rows_are_coalesced_and_trailing_blanks_dropped() {
    // A 120-column screen is mostly blank; carrying it cell by cell would
    // dominate every frame.
    let grid = wire_grid(&snapshot(&["hi        "]));
    assert_eq!(grid.lines[0], vec![ScreenRun::plain("hi")]);
    assert_eq!(grid.cols, 10, "the row is still ten cells wide");
}

#[test]
fn an_empty_snapshot_converts_without_panicking() {
    let grid = wire_grid(&ScreenSnapshot {
        cells: Vec::new(),
        cursor: (0, 0),
        hide_cursor: false,
    });
    assert_eq!(grid.rows, 0);
    assert_eq!(grid.cols, 0);
}


/// A snapshot with one styled run and one plain one, for round-trip tests.
fn styled_snapshot() -> ScreenSnapshot {
    let cell = |ch: &str, bold: bool, fg: vt100::Color| ScreenCell {
        text: CellText::from(ch),
        fg,
        bg: vt100::Color::Default,
        bold,
        italic: false,
        underline: false,
        inverse: false,
    };
    ScreenSnapshot {
        cells: vec![
            vec![
                cell("h", true, vt100::Color::Idx(4)),
                cell("i", true, vt100::Color::Idx(4)),
                cell(" ", false, vt100::Color::Default),
                cell("x", false, vt100::Color::Rgb(1, 2, 3)),
            ],
            vec![
                cell("o", false, vt100::Color::Default),
                cell("k", false, vt100::Color::Default),
                cell(" ", false, vt100::Color::Default),
                cell(" ", false, vt100::Color::Default),
            ],
        ],
        cursor: (1, 2),
        hide_cursor: false,
    }
}

#[test]
fn a_snapshot_survives_a_round_trip_through_the_wire_grid() {
    // The property the whole remote pane rests on: what the daemon's emulator
    // held is what the client's renderer draws. If this drifts, a remote session
    // looks subtly wrong in a way no other test would notice.
    let original = styled_snapshot();
    let restored = super::convert::snapshot_from_grid(&super::convert::wire_grid(&original));
    assert_eq!(restored, original);
}

#[test]
fn dropped_trailing_blanks_come_back_as_a_full_rectangle() {
    // `coalesce_runs` deliberately drops unstyled trailing blanks — the wire
    // carries a row's content. The renderer wants a rectangle, so the inverse
    // has to put them back, or every remote row would be short.
    let original = styled_snapshot();
    let grid = super::convert::wire_grid(&original);
    assert!(
        grid.lines[1]
            .iter()
            .map(|run| run.text.chars().count())
            .sum::<usize>()
            < 4,
        "the fixture's second row must have trailing blanks for this to test anything"
    );
    let restored = super::convert::snapshot_from_grid(&grid);
    assert!(restored.cells.iter().all(|row| row.len() == 4));
}

#[test]
fn a_grid_declaring_more_rows_than_it_carries_is_padded() {
    // Reachable whenever a screen's last rows are empty. Shrinking the pane
    // instead would make it jump every time the harness cleared its output.
    let mut grid = super::convert::wire_grid(&styled_snapshot());
    grid.rows = 5;
    let restored = super::convert::snapshot_from_grid(&grid);
    assert_eq!(restored.cells.len(), 5);
    assert!(restored.cells[4]
        .iter()
        .all(|cell| cell.text.as_str() == " "));
}

#[test]
fn an_empty_grid_yields_an_empty_snapshot_rather_than_panicking() {
    let restored = super::convert::snapshot_from_grid(&medulla::protocol::ScreenGrid {
        cols: 0,
        rows: 0,
        lines: Vec::new(),
        cursor: (0, 0),
        hide_cursor: false,
    });
    assert!(restored.cells.is_empty());
}
