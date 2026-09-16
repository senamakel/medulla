//! Unit tests for the scripted mock board: listing, filtering, vote retallies,
//! comments, submissions, and missing items.

use super::*;

#[tokio::test]
async fn mock_board_lists_and_filters_by_type() {
    let backend = MockBackend::new();
    let all = backend
        .list_feedback(crate::client::FeedbackQuery::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(all.total, 3);
    // `hot` orders by score, so the 24-upvote item leads.
    assert_eq!(all.items[0].id, "fb-1");

    let bugs = backend
        .list_feedback(crate::client::FeedbackQuery {
            kind: Some(crate::client::FeedbackType::Bug),
            ..Default::default()
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bugs.total, 1);
    assert_eq!(bugs.items[0].id, "fb-2");
}

#[tokio::test]
async fn mock_vote_retallies_without_double_counting() {
    let backend = MockBackend::new();
    // fb-2 starts at 11/0 with no vote from us.
    let up = backend.vote_feedback("fb-2".into(), 1).await.unwrap();
    assert_eq!((up.upvote_count, up.downvote_count, up.score), (12, 0, 12));

    // Voting up again must not stack.
    let again = backend.vote_feedback("fb-2".into(), 1).await.unwrap();
    assert_eq!(again.upvote_count, 12);

    // Switching to a downvote moves the tally across, not just adds.
    let down = backend.vote_feedback("fb-2".into(), -1).await.unwrap();
    assert_eq!(
        (down.upvote_count, down.downvote_count, down.score),
        (11, 1, 10)
    );

    // Retracting restores the original tallies.
    let none = backend.vote_feedback("fb-2".into(), 0).await.unwrap();
    assert_eq!(
        (none.upvote_count, none.downvote_count, none.my_vote),
        (11, 0, 0)
    );
}

#[tokio::test]
async fn mock_comment_appends_and_bumps_count() {
    let backend = MockBackend::new();
    let before = backend.feedback_detail("fb-2".into()).await.unwrap();
    assert_eq!(before.comments.len(), 1);

    backend.comment_feedback("fb-2".into(), "me too".into())
        .await
        .unwrap();

    let after = backend.feedback_detail("fb-2".into()).await.unwrap();
    assert_eq!(after.comments.len(), 2);
    assert_eq!(after.comments[1].body, "me too");
    assert_eq!(
        after.feedback.comment_count,
        before.feedback.comment_count + 1
    );
}

#[tokio::test]
async fn mock_submit_prepends_an_accepted_item() {
    let backend = MockBackend::new();
    let result = backend
        .submit_feedback(
            crate::client::FeedbackType::Bug,
            "Tab bar overflows".into(),
            "At 80 columns the tab bar wraps.".into(),
        )
        .await
        .unwrap();
    assert!(result.accepted);

    let page = backend
        .list_feedback(crate::client::FeedbackQuery {
            sort: crate::client::FeedbackSort::New,
            ..Default::default()
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.items[0].title, "Tab bar overflows");
}

#[tokio::test]
async fn mock_board_reports_missing_items() {
    let backend = MockBackend::new();
    assert!(backend.feedback_detail("nope".into()).await.is_err());
    assert!(backend.vote_feedback("nope".into(), 1).await.is_err());
    assert!(backend
        .comment_feedback("nope".into(), "hi".into())
        .await
        .is_err());
}
