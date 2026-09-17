//! Lane-key parsing: recover the wire `(cycleId, taskId)` from a composed lane key.

/// Split a lane task key into its `(cycleId, taskId)` parts. A dispatcher may
/// compose a lane-unique key `"<cycleId>/t:<taskId>"` so two runs delegating the
/// same bare `taskId` never collide; this recovers the wire ids for steering
/// calls (`task.cancel` / `question.answer`). A key with no `/t:` marker is a
/// bare taskId with no cycle, yielding `(None, key)`.
pub fn parse_task_key(key: &str) -> (Option<&str>, &str) {
    match key.split_once("/t:") {
        Some((cycle, task)) => (Some(cycle), task),
        None => (None, key),
    }
}
