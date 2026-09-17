//! Unit tests for the agent lanes, split by responsibility: [`activity`] covers
//! the hub-activity fold; [`render`] covers status classification, key parsing,
//! the row model, and transcript rendering; [`roster`] covers the
//! worker-registry merge that seeds the lanes.

mod activity;
mod render;
mod roster;
