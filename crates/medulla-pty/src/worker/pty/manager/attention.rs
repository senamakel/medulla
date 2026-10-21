//! Timer orchestration for TUI supplied attention classification.

use std::sync::{Arc, Weak};

use crate::attention::{AttentionKind, HarnessAttention};
use crate::sync::lock;

use super::super::handle::SessionHandle;
use super::PtyManager;

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

impl PtyManager {
    pub(super) fn spawn_attention_poller(&self, handle: Weak<SessionHandle>) {
        let inner = Arc::downgrade(&self.inner);
        std::thread::spawn(move || loop {
            std::thread::sleep(INTERVAL);
            let (Some(handle), Some(inner)) = (handle.upgrade(), inner.upgrade()) else {
                return;
            };
            if !handle.is_running() {
                return;
            }
            let now = (inner.now)();
            let (screen, bells) = handle.attention_sample();
            let lifecycle_session = crate::LifecycleSession {
                id: handle.id().to_owned(),
                session_id: handle.session_id(),
                cwd: handle.cwd().to_owned(),
                env: handle.meta.env.clone(),
                grant_session: handle.grant_session().map(str::to_owned),
                provider: handle.provider(),
            };
            let mut state = lock(&handle.attention);
            if now.saturating_sub(state.checked_at) < INTERVAL.as_millis() as i64 * 3 / 4 {
                continue;
            }
            if state.completion_deadline.is_some_and(|deadline| deadline <= std::time::Instant::now()) {
                state.pending_completion_bells = 0;
                state.completion_deadline = None;
                state.generation = state.generation.wrapping_add(1);
            }
            state.checked_at = now;
            let generation = state.generation;
            let seen_bells = state.seen_bells;
            let pending = state.pending_completion_bells;
            drop(state);

            let status = inner.lifecycle.attention_status(
                &lifecycle_session,
                &screen,
                now,
                now.saturating_sub(handle.last_output_at()),
            );
            let unseen = bells.saturating_sub(seen_bells);
            let eligible = unseen.saturating_sub(pending);
            let mut cue = status.cue;
            if eligible > 0 && !status.working && cue.is_none() {
                cue = Some(HarnessAttention::new(AttentionKind::Bell, "harness rang", now));
            }
            let working = status.working && cue.is_none();
            let deferring = working && !status.screen_working && !status.open_ended;
            let mut state = lock(&handle.attention);
            if state.generation != generation {
                continue;
            }
            if !deferring {
                let consumed = unseen.min(state.pending_completion_bells);
                state.pending_completion_bells -= consumed;
                if state.pending_completion_bells == 0 {
                    state.completion_deadline = None;
                }
                state.seen_bells = state.seen_bells.max(bells);
            }
            state.working = working;
            match cue {
                Some(mut next) => {
                    if let Some(current) = &state.cue {
                        if current.kind == next.kind && current.what == next.what {
                            next.since = current.since;
                        } else if !next.supersedes(current) && !deferring {
                            next = current.clone();
                        }
                    }
                    state.cue = Some(next);
                }
                None if !deferring => {
                    state.cue = state
                        .cue
                        .take()
                        .filter(|held| held.kind == AttentionKind::Bell);
                }
                None => {}
            }
        });
    }

    /// Drop `id`'s cue because it has been answered.
    pub fn acknowledge(&self, id: &str) -> bool {
        self.handle(id)
            .is_some_and(|session| session.acknowledge_attention())
    }

    /// What `id` is waiting on the operator for, if anything.
    pub fn attention(&self, id: &str) -> Option<HarnessAttention> {
        self.handle(id)
            .and_then(|session| lock(&session.attention).cue.clone())
    }

    /// How many sessions have a screen-derived attention cue.
    pub fn waiting_count(&self) -> usize {
        self.handles()
            .iter()
            .filter(|session| {
                lock(&session.attention)
                    .cue
                    .as_ref()
                    .is_some_and(|cue| cue.kind.blocks())
            })
            .count()
    }

    /// IDs of sessions with a screen-derived blocking attention cue.
    pub fn waiting_sessions(&self) -> std::collections::HashSet<String> {
        self.handles()
            .into_iter()
            .filter_map(|session| {
                let waits = lock(&session.attention)
                    .cue
                    .as_ref()
                    .is_some_and(|cue| cue.kind.blocks());
                waits.then(|| session.id().to_owned())
            })
            .collect()
    }
}

