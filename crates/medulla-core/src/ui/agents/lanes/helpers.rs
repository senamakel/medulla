//! Private lane and task collection helpers used by the event fold.

use std::collections::HashMap;

use super::types::Lanes;
use crate::ui::agents::types::{AgentLane, AgentRole, TaskState, TaskStatus, TurnBlock};

impl Lanes {
    pub(super) fn new() -> Self {
        Lanes {
            order: Vec::new(),
            map: HashMap::new(),
        }
    }
    pub(super) fn insert(&mut self, lane: AgentLane) {
        if !self.map.contains_key(&lane.key) {
            self.order.push(lane.key.clone());
        }
        self.map.insert(lane.key.clone(), lane);
    }
    pub(super) fn get_mut(&mut self, key: &str) -> Option<&mut AgentLane> {
        self.map.get_mut(key)
    }
    pub(super) fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    pub(super) fn into_ordered(self) -> Vec<AgentLane> {
        let mut map = self.map;
        self.order
            .into_iter()
            .filter_map(|key| map.remove(&key))
            .collect()
    }
}

/// Build a fresh worker-role lane with all optional fields cleared.
pub(super) fn new_agent_lane(key: String, label: String) -> AgentLane {
    AgentLane {
        key,
        label,
        role: AgentRole::Agent,
        turns: Vec::new(),
        last_at: 0,
        tasks: Vec::new(),
        context_tokens: None,
        usage: Default::default(),
        harness_label: None,
        agent_id: None,
        session_id: None,
        parent_agent_id: None,
        descriptor: None,
        active_tasks: 0,
        work: None,
    }
}

/// Find-or-create the task by id, bump its turn count/timestamp, optionally push a
/// block, and return its index.
pub(super) fn touch_task(
    lane: &mut AgentLane,
    task_id: &str,
    at: i64,
    block: Option<TurnBlock>,
) -> usize {
    let idx = match lane.tasks.iter().position(|task| task.task_id == task_id) {
        Some(index) => index,
        None => {
            lane.tasks.push(TaskState {
                task_id: task_id.to_string(),
                status: TaskStatus::Running,
                turns: 0,
                last_at: at,
                turn_blocks: Vec::new(),
                attention: None,
                question_id: None,
                work: None,
            });
            lane.tasks.len() - 1
        }
    };
    let task = &mut lane.tasks[idx];
    task.turns += 1;
    task.last_at = at;
    if let Some(block) = block {
        task.turn_blocks.push(block);
    }
    idx
}

/// Ensure a worker lane exists for `key`, seeding it from `task_agent` when absent.
pub(super) fn ensure_lane(
    workers: &mut Lanes,
    key: &str,
    task_id: &str,
    task_agent: &HashMap<String, String>,
    at: i64,
) {
    if !workers.contains(key) {
        // The fallback is an agent id, which for a host-link peer *is* its
        // address — so it gets the same treatment as one.
        let mut lane = new_agent_lane(
            key.to_string(),
            task_agent
                .get(task_id)
                .map(|id| crate::ui::util::short_if_address(id))
                .unwrap_or_else(|| task_id.to_string()),
        );
        lane.last_at = at;
        if let Some(agent_id) = task_agent.get(task_id) {
            lane.agent_id = Some(agent_id.clone());
        }
        workers.insert(lane);
    }
}

/// Ensure a session lane exists for `key`, tagging its parent machine.
pub(super) fn ensure_session_lane(
    workers: &mut Lanes,
    key: &str,
    agent_id: &str,
    session_id: &str,
    at: i64,
) {
    if !workers.contains(key) {
        // A session id is a UUID — 36 characters of the same opaque noise an
        // address is, and it sits in the same rail measuring the same sidebar.
        // Shortened unconditionally rather than sniffed: we know what this is.
        let mut lane = new_agent_lane(
            key.to_string(),
            format!("↳ {}", crate::ui::util::short_address(session_id)),
        );
        lane.last_at = at;
        lane.session_id = Some(session_id.to_string());
        lane.parent_agent_id = Some(agent_id.to_string());
        workers.insert(lane);
    }
}
