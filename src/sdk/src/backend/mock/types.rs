//! Data types for the `feedback` module.
#[allow(unused_imports)]
use super::*;
/// The scripted board: items plus their comments, keyed by item id.
#[derive(Default)]
pub(super) struct MockBoard {
    /// Board items, in insertion order.
    pub(super) items: Vec<FeedbackItem>,
    /// Comments per item id, oldest first.
    pub(super) comments: Vec<(String, Vec<FeedbackComment>)>,
}
/// The scripted description of one board row, before it is expanded into a
/// [`FeedbackItem`]. Grouping these keeps [`row`] to a single argument.
pub(super) struct Seed<'a> {
    /// The item id.
    pub(super) id: &'a str,
    /// Feature request or bug report.
    pub(super) kind: FeedbackType,
    /// Triage status.
    pub(super) status: FeedbackStatus,
    /// The item title.
    pub(super) title: &'a str,
    /// The item body.
    pub(super) body: &'a str,
    /// Upvotes.
    pub(super) up: i64,
    /// Downvotes.
    pub(super) down: i64,
    /// The demo user's own vote.
    pub(super) my_vote: i8,
    /// How many comments the item has.
    pub(super) comment_count: i64,
    /// The filed GitHub issue number, when the item has been filed.
    pub(super) issue: Option<i64>,
}

/// Format a millisecond timestamp as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub(super) fn iso8601_utc(millis: i64) -> String {
    let secs = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, min, sec) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{ms:03}Z")
}

/// Howard Hinnant's days-from-civil, inverted: days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
