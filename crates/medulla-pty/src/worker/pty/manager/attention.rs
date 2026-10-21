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
            state.checked_at = now;
            let mut cue = inner.lifecycle.classify_attention(&lifecycle_session, &screen, now);
            if bells > state.seen_bells && cue.is_none() {
                cue = Some(HarnessAttention::new(AttentionKind::Bell, "harness rang", now));
            }
            state.seen_bells = bells;
            match cue {
                Some(mut next) => {
                    if let Some(current) = &state.cue {
                        if current.kind == next.kind && current.what == next.what {
                            next.since = current.since;
                        } else if !next.supersedes(current) {
                            continue;
                        }
                    }
                    state.cue = Some(next);
                }
                None => state.cue = None,
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
