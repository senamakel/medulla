//! The event-stream fold: turn the flat event log into the orchestrator lane
//! plus one lane per connected roster agent / anonymous task / peer session.
//! A port of the TS `deriveAgentLanes` essentials.
//!
//! **The reasoning tier is not a lane.** Since the manager refactor the tier
//! between the orchestrator and a harness task is 0..N concurrent *managers*,
//! and the backend deliberately does not stream them: what reaches this client
//! is the orchestrator and the agents it is managing. A manager is an
//! implementation detail of how the orchestrator fanned work out, not a
//! participant the operator talks to or steers. Inference turns on the
//! `reasoning` and `compress` tiers are dropped here rather than folded into a
//! lane nothing feeds. They remain visible verbatim under Settings › Trace.

mod derive;
mod helpers;
mod types;

pub use derive::derive_agent_lanes;
