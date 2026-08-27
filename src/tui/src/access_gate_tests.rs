//! Unit tests for the launch-time access gate's pure parts.

use medulla::access::{Grant, Plan};

use super::startup_note;

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

#[test]
fn a_paid_account_is_not_told_anything() {
    assert_eq!(startup_note(Grant::Paid(Plan::Basic)), None);
    assert_eq!(startup_note(Grant::Paid(Plan::Pro)), None);
}

#[test]
fn an_admin_grant_is_not_announced() {
    assert_eq!(startup_note(Grant::AdminGranted), None);
}

#[test]
fn a_trial_counts_the_days_down() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 5 * DAY_MS,
    });
    assert_eq!(
        note.as_deref(),
        Some("Free trial: 5 days left. Subscribe at https://tinyhumans.ai/pricing")
    );
}

/// Rounding up, so a partial day is never reported as none left.
#[test]
fn a_partial_day_rounds_up_rather_than_reading_as_expired() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 11 * 60 * 60 * 1_000,
    });
    assert_eq!(
        note.as_deref(),
        Some("Free trial: 1 day left. Subscribe at https://tinyhumans.ai/pricing")
    );
}

#[test]
fn the_last_day_is_phrased_in_the_singular() {
    let exactly_one = startup_note(Grant::Trial {
        remaining_ms: DAY_MS,
    });
    assert!(exactly_one.unwrap().starts_with("Free trial: 1 day left"));

    let two = startup_note(Grant::Trial {
        remaining_ms: 2 * DAY_MS,
    });
    assert!(two.unwrap().starts_with("Free trial: 2 days left"));
}

#[test]
fn a_full_week_reads_as_seven_days() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 7 * DAY_MS,
    });
    assert!(note.unwrap().starts_with("Free trial: 7 days left"));
}
