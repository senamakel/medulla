impl App {
    /// Deliver the fetched account usage payload (None = backend unavailable).
    ///
    /// Returns the subscription sample that was waiting for this payload, or a
    /// fresh sample when an independently loaded payload makes an enabled
    /// TinyHumans meter newly readable.
    pub fn set_account_usage(&mut self, data: Option<serde_json::Value>) -> Option<Cmd> {
        self.account_usage = data;
        if self.subscriptions_waiting_for_account {
            return Some(self.begin_subscription_sample());
        }
        if self
            .loaded
            .config
            .subscriptions
            .source_enabled(medulla::config::MeterSource::Tinyhumans)
        {
            if self.subscriptions_loading {
                self.subscriptions_refresh_pending = true;
                return None;
            }
            return Some(self.begin_subscription_sample());
        }
        None
    }

    /// Release a subscription refresh whose account-usage read failed.
    ///
    /// The last successful account payload remains available to the Usage page
    /// and the TinyHumans meter; a transient failure should not turn a visible
    /// balance into an unavailable row.
    pub fn usage_load_failed(&mut self) -> Option<Cmd> {
        self.subscriptions_waiting_for_account
            .then(|| self.begin_subscription_sample())
    }

    /// Deliver a fresh subscription-usage sample and start a queued refresh.
    pub fn set_subscriptions(
        &mut self,
        snapshot: medulla::subscriptions::SubscriptionSnapshot,
    ) -> Option<Cmd> {
        self.subscriptions = snapshot;
        self.subscriptions_loading = false;
        if self.subscriptions_refresh_pending {
            self.subscriptions_refresh_pending = false;
            return Some(self.begin_subscription_refresh());
        }
        None
    }

    /// The latest subscription-usage readings. Render/test seam.
    pub fn subscriptions(&self) -> &medulla::subscriptions::SubscriptionSnapshot {
        &self.subscriptions
    }

    /// The command that re-samples the meters, when one is due and none is
    /// already running.
    ///
    /// Called on the event loop's tick. Returns `None` while every meter is off
    /// — the sample would be empty and, more to the point, no request should
    /// leave the machine for a meter nobody asked for.
    pub fn due_subscription_refresh(&mut self) -> Option<Cmd> {
        let config = &self.loaded.config.subscriptions;
        if self.subscriptions_loading || !config.any_enabled() {
            return None;
        }
        let now = medulla::clock::now_millis() / 1000;
        if !self
            .subscriptions
            .is_stale(now, config.sampling_interval_seconds())
        {
            return None;
        }
        Some(self.begin_subscription_refresh())
    }

    /// Force a re-sample regardless of when the last one landed.
    ///
    /// What `r` on the Subscriptions page does: the automatic cadence is sized
    /// for windows that move over hours, and "tell me now" is the case it does
    /// not cover.
    pub(in crate::ui::app) fn refresh_subscriptions(&mut self) -> Option<Cmd> {
        if self.subscriptions_loading {
            // Account usage is fetched before the subscription command clones
            // its config, so edits made during that stage are already included.
            // Once the provider sample starts, however, one follow-up is needed.
            if !self.subscriptions_waiting_for_account {
                self.subscriptions_refresh_pending = true;
            }
            return None;
        }
        Some(self.begin_subscription_refresh())
    }

    /// Start a refresh, loading a current account payload for TinyHumans first.
    fn begin_subscription_refresh(&mut self) -> Cmd {
        self.subscriptions_loading = true;
        if self
            .loaded
            .config
            .subscriptions
            .source_enabled(medulla::config::MeterSource::Tinyhumans)
        {
            self.subscriptions_waiting_for_account = true;
            Cmd::LoadUsage
        } else {
            self.begin_subscription_sample()
        }
    }

    /// Start provider sampling with the account payload currently held in App.
    fn begin_subscription_sample(&mut self) -> Cmd {
        self.subscriptions_loading = true;
        self.subscriptions_waiting_for_account = false;
        Cmd::LoadSubscriptions(self.account_usage.clone())
    }

    /// Set the status-line text.
    pub fn set_status(&mut self, s: impl Into<String>) {
        // A destructive confirmation is valid only while its question remains
        // visible. Any asynchronous status replacement cancels it.
        self.kill_armed = None;
        self.workflow_delete_armed = None;
        // The harness close question is the same kind of promise: it is only
        // answerable while the sentence asking it is the one on screen.
        self.harness_close_armed = None;
        self.status = s.into();
    }

    /// Show and arm the session-kill confirmation as one invariant-preserving
    /// state transition.
    pub(super) fn arm_kill(&mut self, target: (String, String)) {
        self.set_status("Kill this session? y confirm · any other key cancels");
        self.kill_armed = Some(target);
    }

    /// Show the workflow deletion confirmation and retain the exact record it names.
    #[cfg(feature = "workflows")]
    pub(super) fn arm_workflow_delete(&mut self, id: String, name: String) {
        self.set_status(format!("Delete {name}? Choose Delete or Esc"));
        self.workflow_delete_armed = Some((id, name));
    }

    /// Show and arm the "close this harness" confirmation for `session`.
    ///
    /// Set after the status line, never before: [`set_status`](Self::set_status)
    /// disarms, so arming first would leave the question visible and unanswerable.
    pub(super) fn arm_harness_close(&mut self, session: String) {
        self.set_status("Close this harness? y confirm · any other key cancels");
        self.harness_close_armed = Some(session);
    }

    /// Replace the Context-tab chunks.
    pub fn set_contexts(&mut self, c: Vec<ContextItem>) {
        self.contexts = c;
    }

    /// Open the resume picker with `chats`, or report that there is nothing to
    /// resume.
    pub fn open_resume(&mut self, chats: Vec<crate::ui::chat_store::MainChatSummary>) {
        if chats.is_empty() {
            self.set_status("No saved chats to resume.");
        } else {
            self.resume_picker = Some(ResumePicker { chats, index: 0 });
            self.set_status("Resume: ↑/↓ select · Enter load · Esc cancel");
        }
    }

    /// The active tab name.
    pub fn tab(&self) -> &'static str {
        TABS[self.tab_index]
    }

    /// The lazy-load command a freshly entered tab (or Settings subpage) needs.
    ///
    /// Context, Feedback, and Usage all fetch on entry; since they are now
    /// Settings subpages rather than tabs, the Settings arm dispatches on the
    /// active subpage.
    pub(super) fn tab_enter_cmd(&mut self) -> Option<Cmd> {
        // No tab needs a focus nudge on entry: the rail forwards typing, so a
        // printable key moves focus to the composer and lands the character
        // there, and nothing is lost by not starting in it.
        match self.tab() {
            "Feedback" => Some(Cmd::LoadFeedback(self.feedback.query.clone())),
            // The diff pane is the Changes surface now, so entering the tab
            // re-loads its git data the way the removed Changes tab did on
            // entry: a repo that changed while the operator was on another tab
            // would otherwise stay stale beneath the open diff.
            "Sessions" => {
                if self.pane_view == PaneView::Diff {
                    self.refresh_changes();
                }
                None
            }
            // The workflow store is files on this machine, so entering the tab
            // reads them rather than asking the runtime for anything — which is
            // why this arm returns no command and does the work here.
            #[cfg(feature = "workflows")]
            "Workflows" => {
                self.reload_workflows();
                None
            }
            "Settings" => match self.settings_index {
                SP_USAGE => Some(Cmd::LoadUsage),
                SP_HARNESSES => Some(Cmd::LoadHarnesses {
                    sources: self.loaded.sources.clone(),
                    medulla_home: self.medulla_home.clone(),
                }),
                SP_SUBSCRIPTIONS => self.refresh_subscriptions(),
                SP_CONTEXT => Some(Cmd::InspectContext),
                SP_FEEDBACK => Some(Cmd::LoadFeedback(self.feedback.query.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Derive the current agent lanes from the snapshot, harness, and roster.
    ///
    /// The roster is the snapshot's (what the backend advertises, plus any
    /// host-link peers the observation overlays) merged with the runtime's own
    /// worker registry. Both are needed: a worker added at runtime lives only in
    /// the registry — which is what resolves a delegated task's address — so
    /// reading the snapshot alone left a live, dispatchable worker off this tab.
    pub(super) fn lanes(&self) -> Vec<AgentLane> {
        let roster = merge_host_roster(&self.fleet_roster(), &self.runtime.workers());
        let mut lanes = derive_agent_lanes(&self.snapshot.events, &self.loaded.harness(), &roster);
        // The snapshot's events come from the backend, whose vocabulary says
        // nothing about delegated tasks — so a busy agent renders idle unless
        // the activity the hub observed locally is folded in.
        merge_host_activity(&mut lanes, &self.runtime.worker_activity());
        lanes
    }

    /// The commands offered for the current draft, or `None` when the peek is
    /// closed.
    ///
    /// Open only on the conversation surface: the composer is the only place a
    /// command can be typed, so offering the catalog anywhere else would be
    /// advertising an input that is not there.
    pub(super) fn command_suggestions(&self) -> Option<Vec<&'static CommandSpec>> {
        if self.tab() != "Sessions" || self.prompt.is_some() {
            return None;
        }
        crate::ui::command::suggestions(&self.draft.text)
    }

    /// The highlighted command's name, while the peek is open.
    /// Test/inspection seam.
    pub fn selected_command(&self) -> Option<String> {
        self.peeked_command().map(|spec| spec.name.to_string())
    }

    /// The command the peek would complete to, if it is open on a match.
    pub(super) fn peeked_command(&self) -> Option<&'static CommandSpec> {
        let specs = self.command_suggestions()?;
        specs
            .get(self.command_index.min(specs.len().saturating_sub(1)))
            .copied()
    }

    /// Scroll the transcript the Sessions cursor is reading.
    pub(super) fn scroll_transcript(&mut self, up: bool, step: usize) {
        let target = &mut self.agent_scroll;
        *target = if up {
            target.saturating_add(step)
        } else {
            target.saturating_sub(step)
        };
    }

    /// The capacity the fleet surfaces render.
    ///
    /// The runtime's own declaration wins; the operator's `[fleet]` config is
    /// the fallback, so a declared chain is still visible on a runtime that
    /// reports none (the mock, or a backend that has not answered yet). They are
    /// not merged: two half-descriptions of one fleet interleaved would be
    /// harder to trust than whichever one is authoritative.
    ///
    pub(super) fn fleet_capacity(&self) -> medulla::runtime::CapacitySnapshot {
        let mut declared = if self.snapshot.capacity.is_empty() {
            self.loaded.config.fleet.capacity()
        } else {
            self.snapshot.capacity.clone()
        };
        // Last resort, and opt-in only: with `MEDULLA_DEMO_FLEET` set and nothing
        // real declared, stand in a small fake fleet so the surfaces can be
        // exercised without a backend. It never overrides a real reading.
        if declared.is_empty() && medulla::runtime::demo_fleet_requested() {
            declared = medulla::runtime::demo_capacity();
        }
        // The locally registered peers are hosts too, and they are frequently
        // the only capacity a hub-backed session has. Declared records win on a
        // collision; the registry only ever adds machines nothing else named.
        merge_capacity(&declared, &registry_capacity(&self.runtime.workers()))
    }

    /// The agent identities the fleet surfaces place.
    ///
    /// The snapshot roster, or the stand-in one when `MEDULLA_DEMO_FLEET` is set
    /// and the runtime has none. Locally registered peers are deliberately *not*
    /// merged in here: a peer is a *host*, and it reaches these surfaces through
    /// [`fleet_capacity`](App::fleet_capacity) as one. Listing it here too would
    /// show the same machine twice — once as capacity, once as an agent with
    /// nowhere to live.
    pub(super) fn fleet_roster(&self) -> Vec<medulla::runtime::AgentDescriptor> {
        if self.snapshot.roster.is_empty() && medulla::runtime::demo_fleet_requested() {
            return medulla::runtime::demo_agents();
        }
        self.snapshot.roster.clone()
    }

    /// The index of the active thread in the snapshot's thread list.
    pub(super) fn active_thread_idx(&self) -> usize {
        self.snapshot
            .threads
            .iter()
            .position(|t| t.id == self.snapshot.active_thread_id)
            .unwrap_or(0)
    }

    /// Events-length change signal, so the loop can re-inspect context.
    pub fn events_changed(&mut self) -> bool {
        let n = self.snapshot.events.len();
        let changed = n != self.last_events_len;
        self.last_events_len = n;
        changed
    }
}
