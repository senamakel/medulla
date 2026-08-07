//! Tests for path compaction in the Agents rail.

use unicode_width::UnicodeWidthStr;

use super::wrap::{flow_path, short_home, wrap_path};

#[test]
fn a_path_segment_of_wide_characters_hard_cuts_by_display_column() {
    let out = flow_path("任务一二三四五六七八九十", 10);
    assert!(out.iter().all(|line| line.width() <= 10));
    assert_eq!(out.concat(), "任务一二三四五六七八九十");
}

#[test]
fn an_unbreakable_final_segment_keeps_its_own_tail() {
    let out = wrap_path(
        "~/work/a/very-long-checkout-name-that-alone-overruns-the-budget-abcdefg",
        12,
        2,
    );
    assert!(out.len() <= 2);
    let joined = out.concat();
    assert!(joined.ends_with("abcdefg"));
    assert!(joined.starts_with('…'));
}

#[test]
fn homes_compact_only_their_own_path_prefix() {
    let home = Some("/Users/dev");
    assert_eq!(short_home("/Users/dev/work/repo", home), "~/work/repo");
    assert_eq!(short_home("/Users/dev", home), "~");
    assert_eq!(short_home("/Users/developer/x", home), "/Users/developer/x");
    assert_eq!(short_home("/srv/repos/auth", None), "/srv/repos/auth");
}

#[test]
fn a_windows_home_collapses_on_its_own_separator() {
    let home = Some("C:\\Users\\dev");
    assert_eq!(
        short_home("C:\\Users\\dev\\work\\repo", home),
        "~\\work\\repo"
    );
    assert_eq!(short_home("D:\\src\\other", home), "D:\\src\\other");
}
