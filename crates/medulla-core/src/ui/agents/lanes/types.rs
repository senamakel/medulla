//! Data types for the `lanes` module.
use std::collections::HashMap;

use crate::ui::agents::types::AgentLane;
/// Insertion-ordered lane collection.
pub(super) struct Lanes {
    pub(super) order: Vec<String>,
    pub(super) map: HashMap<String, AgentLane>,
}
