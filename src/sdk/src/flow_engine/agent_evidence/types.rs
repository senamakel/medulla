//! Prompt evidence storage for one workflow engine invocation.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use serde_json::Value;

use crate::workflows::RunStep;

use super::NODE_ID_FIELD;

/// Resolved agent prompts waiting to be attached to completed run steps.
#[derive(Debug, Default)]
pub(crate) struct AgentEvidence {
    prompts: Mutex<HashMap<String, VecDeque<String>>>,
}

impl AgentEvidence {
    /// Record the prompt from one resolved engine request, when it carries a tag.
    pub(crate) fn record(&self, request: &Value, prompt: &str) {
        let Some(node_id) = request.get(NODE_ID_FIELD).and_then(Value::as_str) else {
            return;
        };
        self.prompts
            .lock()
            .expect("agent evidence lock")
            .entry(node_id.to_string())
            .or_default()
            .push_back(prompt.to_string());
    }

    /// Attach prompts to their corresponding persisted steps in completion order.
    pub(crate) fn attach(&self, steps: &mut [RunStep]) {
        let mut prompts = self.prompts.lock().expect("agent evidence lock");
        let mut remaining = HashMap::<String, usize>::new();
        for step in steps.iter() {
            *remaining.entry(step.node_id.clone()).or_default() += 1;
        }
        for step in steps {
            let Some(node_prompts) = prompts.get_mut(&step.node_id) else {
                continue;
            };
            let steps_left = remaining.get_mut(&step.node_id).expect("step was counted");
            let take = if *steps_left == 1 {
                node_prompts.len()
            } else {
                1.min(node_prompts.len())
            };
            let values = node_prompts
                .drain(..take)
                .map(Value::String)
                .collect::<Vec<_>>();
            step.input = match values.len() {
                0 => None,
                1 => values.into_iter().next(),
                _ => Some(Value::Array(values)),
            };
            *steps_left -= 1;
        }
    }
}
