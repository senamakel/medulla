//! Pure translation from core wire types into the render snapshot.
//!
//! Kept as free functions with no core and no I/O, deliberately. The fold is
//! where a migration like this actually goes wrong: not with a crash, but with
//! lanes that quietly stop counting because a field moved. Pure functions can
//! be tested exhaustively against hand-written inputs, which is the only way
//! that class of bug gets caught.

use openhuman_core::embed::{RosterWorker, SessionSummary};

use crate::runtime::types::{AgentDescriptor, ThreadSummary};

/// Fold the core's worker roster into render descriptors.
pub fn roster(workers: Vec<RosterWorker>) -> Vec<AgentDescriptor> {
    workers
        .into_iter()
        .map(|w| AgentDescriptor {
            id: w.registry_id,
            name: w.label,
            description: w.description,
            availability: w.availability,
            // The roster carries no placement or provenance, and the render
            // layer reads absent as "not declared". Defaulting the rest keeps
            // this fold honest about what the core actually told us.
            ..AgentDescriptor::default()
        })
        .collect()
}

/// Fold the core's session list into thread summaries.
///
/// The counters (`turns`, `running_tasks`, `attention`) stay zero: the session
/// list carries no per-thread activity, and inventing a value would render as
/// real data. They fill in when the event stream is wired.
pub fn threads(sessions: Vec<SessionSummary>) -> Vec<ThreadSummary> {
    sessions
        .into_iter()
        .map(|s| ThreadSummary {
            id: s.session_id,
            name: s.title.unwrap_or_default(),
            running: false,
            turns: 0,
            running_tasks: 0,
            attention: 0,
        })
        .collect()
}
