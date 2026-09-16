//! A scripted, in-memory [`Backend`] for the offline demo and the test suites.
//!
//! `medulla --mock` lands here, so this *is* the offline demo of the Feedback
//! tab. The board is stateful rather than a frozen fixture: votes retally and
//! comments persist for the life of the process, so the tab's controls can be
//! exercised without a backend. There is no account behind it, so usage reads
//! as unavailable and logout is a no-op.

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use super::Backend;
use crate::client::{
    FeedbackComment, FeedbackDetail, FeedbackGithub, FeedbackItem, FeedbackPage, FeedbackQuery,
    FeedbackStatus, FeedbackSubmission, FeedbackType,
};

mod types;
use types::{MockBoard, Seed};

/// The offline stand-in: a scripted board and nothing else.
#[derive(Clone)]
pub struct MockBackend {
    /// Scripted feedback board, mutated in place by votes and comments so the
    /// tab's controls work end to end without a backend.
    board: Arc<Mutex<MockBoard>>,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    /// A backend seeded with the demo board.
    pub fn new() -> Self {
        Self {
            board: Arc::new(Mutex::new(MockBoard::demo())),
        }
    }
}

/// Current wall-clock time in milliseconds.
fn now_millis() -> i64 {
    crate::clock::now_millis()
}

/// Mint an id of the form `{prefix}-{millis}-{hex}`.
fn gen_id(prefix: &str) -> String {
    // Cheap, dependency-free pseudo-random suffix derived from the clock.
    let suffix = (now_millis() as u64)
        .wrapping_mul(2654435761)
        .rotate_left(13) as u16;
    format!("{prefix}-{}-{suffix:04x}", now_millis())
}

impl Default for Seed<'_> {
    fn default() -> Self {
        Seed {
            id: "",
            kind: FeedbackType::Feature,
            status: FeedbackStatus::Open,
            title: "",
            body: "",
            up: 0,
            down: 0,
            my_vote: 0,
            comment_count: 0,
            issue: None,
        }
    }
}

/// Expand a [`Seed`] into a scripted board row.
fn row(seed: Seed<'_>) -> FeedbackItem {
    FeedbackItem {
        id: seed.id.into(),
        kind: seed.kind,
        title: seed.title.into(),
        body: seed.body.into(),
        status: seed.status,
        created_by_name: Some("demo user".into()),
        upvote_count: seed.up,
        downvote_count: seed.down,
        score: seed.up - seed.down,
        comment_count: seed.comment_count,
        github: seed.issue.map(|n| FeedbackGithub {
            issue_number: Some(n),
            issue_url: Some(format!(
                "https://github.com/tinyhumansai/medulla/issues/{n}"
            )),
        }),
        my_vote: seed.my_vote,
        created_at: types::iso8601_utc(now_millis()),
    }
}

/// One scripted comment.
fn comment(id: &str, who: &str, body: &str) -> FeedbackComment {
    FeedbackComment {
        id: id.into(),
        user_name: Some(who.into()),
        body: body.into(),
        created_at: types::iso8601_utc(now_millis()),
    }
}

impl MockBoard {
    /// The board the offline demo starts with.
    pub(super) fn demo() -> Self {
        let items = vec![
            row(Seed {
                id: "fb-1",
                kind: FeedbackType::Feature,
                status: FeedbackStatus::Planned,
                title: "Filter the Sessions rail by host",
                body: "A fleet across three machines is hard to read in one list. \
                       Filtering the rail by host would make a busy day far easier to \
                       follow.",
                up: 24,
                down: 1,
                my_vote: 1,
                comment_count: 2,
                issue: Some(412),
            }),
            row(Seed {
                id: "fb-2",
                kind: FeedbackType::Bug,
                status: FeedbackStatus::Open,
                title: "Reattaching a session loses the scroll position",
                body: "After Ctrl-] into a session and back out, the transcript pane \
                       lands at the top instead of where it was.",
                up: 11,
                comment_count: 1,
                ..Default::default()
            }),
            row(Seed {
                id: "fb-3",
                kind: FeedbackType::Feature,
                status: FeedbackStatus::Completed,
                title: "Persist theme choice across restarts",
                body: "Appearance changes should survive a restart.",
                up: 8,
                down: 2,
                my_vote: -1,
                issue: Some(377),
                ..Default::default()
            }),
        ];
        let comments = vec![
            (
                "fb-1".to_string(),
                vec![
                    comment("c1", "avery", "Would pair well with per-host usage meters."),
                    comment(
                        "c2",
                        "demo user",
                        "Agreed — the Hosts tab already knows the machines.",
                    ),
                ],
            ),
            (
                "fb-2".to_string(),
                vec![comment("c3", "jules", "Reproduced on 0.3.1.")],
            ),
            ("fb-3".to_string(), Vec::new()),
        ];
        MockBoard { items, comments }
    }

    /// The comment list for `id`, inserting an empty one if absent.
    fn comments_mut(&mut self, id: &str) -> &mut Vec<FeedbackComment> {
        if !self.comments.iter().any(|(k, _)| k == id) {
            self.comments.push((id.to_string(), Vec::new()));
        }
        self.comments
            .iter_mut()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v)
            .expect("just inserted")
    }
}

impl MockBackend {
    /// Apply a vote locally, mirroring the backend's retally-from-scratch rule:
    /// a repeated vote is not additive, and `0` retracts.
    fn mock_vote(&self, id: &str, value: i8) -> anyhow::Result<FeedbackItem> {
        let mut board = self.board.lock().unwrap();
        let item = board
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("Feedback not found"))?;

        // Undo the caller's previous vote, then apply the new one.
        match item.my_vote {
            1 => item.upvote_count -= 1,
            -1 => item.downvote_count -= 1,
            _ => {}
        }
        match value {
            1 => item.upvote_count += 1,
            -1 => item.downvote_count += 1,
            _ => {}
        }
        item.my_vote = value;
        item.score = item.upvote_count - item.downvote_count;
        Ok(item.clone())
    }

    /// Append a comment locally and bump the item's comment count.
    fn mock_comment(&self, id: &str, body: &str) -> anyhow::Result<FeedbackComment> {
        let mut board = self.board.lock().unwrap();
        if !board.items.iter().any(|i| i.id == id) {
            return Err(anyhow::anyhow!("Feedback not found"));
        }
        let created = comment(&gen_id("c"), "you", body);
        board.comments_mut(id).push(created.clone());
        if let Some(item) = board.items.iter_mut().find(|i| i.id == id) {
            item.comment_count += 1;
        }
        Ok(created)
    }

    /// Add a submitted item to the head of the scripted board.
    fn mock_submit(&self, kind: FeedbackType, title: &str, body: &str) -> FeedbackSubmission {
        let item = row(Seed {
            id: &gen_id("fb"),
            kind,
            title,
            body,
            ..Default::default()
        });
        self.board.lock().unwrap().items.insert(0, item.clone());
        FeedbackSubmission {
            accepted: true,
            reason: "accepted (mock backend — nothing was sent to the backend)".into(),
            feedback: Some(item),
        }
    }

    /// Filter, sort, and paginate the scripted board like the backend does.
    fn mock_list(&self, query: &FeedbackQuery) -> FeedbackPage {
        let board = self.board.lock().unwrap();
        let mut items: Vec<FeedbackItem> = board
            .items
            .iter()
            .filter(|i| query.kind.is_none_or(|k| k == i.kind))
            .filter(|i| query.status.is_none_or(|s| s == i.status))
            .cloned()
            .collect();
        // `hot` and `top` both order by score here; the mock has no time decay.
        match query.sort {
            crate::client::FeedbackSort::New => {}
            _ => items.sort_by_key(|i| std::cmp::Reverse(i.score)),
        }
        let total = items.len() as i64;
        let skip = ((query.page.max(1) - 1) * query.limit.max(1)) as usize;
        let items = items
            .into_iter()
            .skip(skip)
            .take(query.limit.clamp(1, 100) as usize)
            .collect();
        FeedbackPage { items, total }
    }

    /// One scripted item with its comments.
    fn mock_detail(&self, id: &str) -> anyhow::Result<FeedbackDetail> {
        let board = self.board.lock().unwrap();
        let feedback = board
            .items
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Feedback not found"))?;
        let comments = board
            .comments
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        Ok(FeedbackDetail { feedback, comments })
    }
}

impl Backend for MockBackend {
    fn describe(&self) -> String {
        "mock backend (offline demo)".to_string()
    }

    fn list_feedback(
        &self,
        query: FeedbackQuery,
    ) -> BoxFuture<'static, anyhow::Result<Option<FeedbackPage>>> {
        let page = self.mock_list(&query);
        Box::pin(async move { Ok(Some(page)) })
    }

    fn feedback_detail(&self, id: String) -> BoxFuture<'static, anyhow::Result<FeedbackDetail>> {
        let result = self.mock_detail(&id);
        Box::pin(async move { result })
    }

    fn vote_feedback(
        &self,
        id: String,
        value: i8,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackItem>> {
        let result = self.mock_vote(&id, value);
        Box::pin(async move { result })
    }

    fn comment_feedback(
        &self,
        id: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackComment>> {
        let result = self.mock_comment(&id, &body);
        Box::pin(async move { result })
    }

    fn submit_feedback(
        &self,
        kind: FeedbackType,
        title: String,
        body: String,
    ) -> BoxFuture<'static, anyhow::Result<FeedbackSubmission>> {
        let result = self.mock_submit(kind, &title, &body);
        Box::pin(async move { Ok(result) })
    }
}

#[cfg(test)]
mod tests;
