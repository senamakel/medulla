
#[test]
fn a_combining_sequence_stays_one_cell_across_the_wire() {
    // `cell.contents()` can be several scalars for one cell — `e` plus a
    // combining acute. Splitting the run by `char` on the way back emitted two
    // cells where the emulator had one, shifting every column after it and
    // letting the row's resize truncate real content off the end.
    use super::super::pty::{CellText, ScreenCell, ScreenSnapshot};
    let cell = |text: &str| ScreenCell {
        text: CellText::from(text),
        fg: vt100::Color::Default,
        bg: vt100::Color::Default,
        bold: false,
        italic: false,
        underline: false,
        inverse: false,
    };
    let original = ScreenSnapshot {
        // "e" + U+0301 in one cell, then two ordinary ones.
        cells: vec![vec![cell("e\u{301}"), cell("o"), cell("k")]],
        cursor: (0, 0),
        hide_cursor: false,
    };

    let restored = super::convert::snapshot_from_grid(&super::convert::wire_grid(&original));
    assert_eq!(
        restored.cells[0].len(),
        3,
        "the combining mark must not create a fourth cell: {:?}",
        restored.cells[0]
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(restored, original);
}

#[test]
fn a_wide_glyph_keeps_its_continuation_cell() {
    // vt100 represents a wide glyph as a leading cell plus an empty
    // continuation, so both cross as cells and both must come back — otherwise
    // every column to the right of a CJK character shifts left by one.
    use super::super::pty::{CellText, ScreenCell, ScreenSnapshot};
    let cell = |text: &str| ScreenCell {
        text: CellText::from(text),
        fg: vt100::Color::Default,
        bg: vt100::Color::Default,
        bold: false,
        italic: false,
        underline: false,
        inverse: false,
    };
    let original = ScreenSnapshot {
        // The blank after the glyph is vt100's continuation cell.
        cells: vec![vec![cell("私"), cell(" "), cell("x")]],
        cursor: (0, 0),
        hide_cursor: false,
    };
    let restored = super::convert::snapshot_from_grid(&super::convert::wire_grid(&original));
    assert_eq!(restored.cells[0].len(), 3);
    assert_eq!(restored.cells[0][0].text.as_str(), "私");
    assert_eq!(restored.cells[0][2].text.as_str(), "x");
    assert_eq!(restored, original);
}
