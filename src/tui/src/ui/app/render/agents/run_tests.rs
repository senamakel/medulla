//! Regression coverage for rendering the selected workflow run.

use ratatui::text::{Line, Span};

use super::run::wrapped_rows;

#[test]
fn wrapped_rows_counts_indented_live_output() {
    // The paragraph keeps this prefix because it uses `Wrap { trim: false }`.
    // The two leading spaces, bullet and following separator leave room for
    // only six of the eight letters at ten columns.
    let lines = [Line::from(Span::raw("  · abcdefgh"))];

    assert_eq!(wrapped_rows(&lines, 10), 2);
}
