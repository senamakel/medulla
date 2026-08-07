//! Regression coverage for the Status line settings layout.

use ratatui::text::Line;

use super::footer::rendered_height;

#[test]
fn footer_height_accounts_for_wrapped_detail_lines() {
    let lines = vec![Line::from(
        "A detailed footer sentence that exceeds the pane width",
    )];

    assert_eq!(rendered_height(&lines, 10), 6);
}
