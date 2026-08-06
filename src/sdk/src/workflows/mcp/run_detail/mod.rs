//! Joining a run's durable record to the harness sessions it has in flight.
//!
//! `workflow_run_get` reads the store, and the store only learns about a node
//! once that node has *finished*: a step is appended when the engine reports it
//! settled. So for the twenty minutes an `agent` node spends being a whole
//! coding session, the record says nothing about it at all — the caller sees a
//! run that is `running` with one fewer step than it expects and has no way to
//! tell a harness working from a harness wedged.
//!
//! The missing half is not in the store, and it is not in the engine either. It
//! is in the hub, which watched the dispatch go out and is watching the frames
//! come back. This module reaches for it.
//!
//! # The join
//!
//! Every `agent` dispatch is tagged `wf:{run_id}:{route}#{sequence}` (see
//! [`crate::flow_engine::caps::agent`]), and the hub's activity log records
//! which worker each outstanding task id is running on. That id is therefore
//! the whole of the correlation: filter the roster's `running` lists by the
//! `wf:{run_id}:` prefix and what is left is exactly this run's live harness
//! sessions, attributed to machines.
//!
//! # What this cannot see
//!
//! The harness *transcript* — what the session is typing, which files it has
//! touched, how far through it thinks it is. The hub carries a work snapshot on
//! each frame it observes, but no control-plane op exposes the activity log, so
//! the MCP server (a subprocess with no hub of its own) cannot read it. The
//! honest answer is therefore "this worker is running this step of your run",
//! not a progress bar, and the tool says so rather than implying more.
//!
//! Neither can it see a run executing in another process. The cancellation
//! registry is process-local, so `executingHere` is a fact about *this* server,
//! and a run started from the TUI reads as not executing here even while it is
//! plainly alive — which is why the fleet half is reported independently of it.

mod detail;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use detail::detail;
