//! Which tools a turn is served, and the journal and proposal verbs.
//!
//! The important thing here is [`ToolMode`]. An evolution turn must not be able
//! to edit a saved graph, and the cheapest honest way to guarantee that is not
//! to serve it the verb. A standing instruction in the prompt is a request that
//! one confused turn can talk itself past; a tool that is absent from
//! `tools/list` and refused by `tools/call` is a fact.
//!
//! Note what is *not* here: accepting and rejecting a proposal. Those are the
//! operator's decision, and the cheapest enforcement of that is that no agent
//! has a verb for them.

use serde_json::{json, Value};

/// Restricts evolution writes to the workflow being reviewed.
pub const TOOL_SCOPE_ENV: &str = "MEDULLA_WORKFLOW_SCOPE";

/// The definitions for the journal and proposal tools.
pub fn definitions(schema: impl Fn(Value, &[&str]) -> Value) -> Vec<Value> {
    vec![
        json!({
            "name": "workflow_notes",
            "description":
                "What this host has already learned about a workflow: observations from past \
                 runs, constraints an operator stated, fixes that were applied, and changes \
                 that were rejected. Read this before proposing anything — a change that was \
                 already turned down should not come back without new evidence.",
            "inputSchema": schema(
                json!({ "id": { "type": "string", "description": "The workflow id." } }),
                &["id"],
            ),
        }),
        json!({
            "name": "workflow_note_add",
            "description":
                "Record something you learned about a workflow, so the next review starts from \
                 it rather than re-deriving it. Write one even when you propose no change: \
                 ruling something out is worth keeping. Keep a note to a claim — what you \
                 observed, or what you concluded and from which runs — rather than a summary \
                 of your turn.",
            "inputSchema": schema(
                json!({
                    "id": { "type": "string", "description": "The workflow the note is about." },
                    "kind": {
                        "type": "string",
                        "enum": ["observation", "hypothesis", "constraint", "fix", "rejection"],
                        "description":
                            "observation: something that happened, stated without explanation. \
                             hypothesis: a proposed cause, not yet confirmed. constraint: a rule \
                             any future change must respect. fix: a change that was made and what \
                             it was for. rejection: a change considered and turned down.",
                    },
                    "text": { "type": "string", "description": "The note itself." },
                    "runIds": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The runs this note is evidence from, if any.",
                    },
                    "supersedes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description":
                            "Ids of earlier notes this one replaces — a hypothesis you have \
                             now disproved, say. Superseded notes stay in the history an \
                             operator reads but stop being shown to future reviews, so use \
                             this instead of writing a contradiction and leaving both.",
                    }
                }),
                &["id", "kind", "text"],
            ),
        }),
        json!({
            "name": "workflow_proposals",
            "description":
                "Changes that have been proposed for a workflow, with what verifying each one \
                 found and whether an operator accepted it.",
            "inputSchema": schema(
                json!({ "id": { "type": "string", "description": "The workflow id." } }),
                &["id"],
            ),
        }),
        json!({
            "name": "workflow_propose",
            "description":
                "Propose a change to a workflow's graph for an operator to accept. The ops are \
                 the same patch language as workflow_apply_ops, but nothing is written to the \
                 graph: the proposal is applied to a copy, validated, and dry-run, and the \
                 result is stored for review. A proposal that fails those checks is still kept, \
                 with the reason — so make the rationale say what evidence led you here, not \
                 just what the patch does.",
            "inputSchema": schema(
                json!({
                    "id": { "type": "string", "description": "The workflow to change." },
                    "rationale": {
                        "type": "string",
                        "description":
                            "Why this change, and what evidence points to it. What an operator \
                             reads before deciding.",
                    },
                    "ops": { "type": "array", "description": "The graph ops to propose." },
                    "runIds": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The runs that motivated this.",
                    },
                    "noteIds": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description":
                            "The journal notes that motivated this. Populate this when the \
                             rationale relies on evidence or constraints from workflow_notes.",
                    }
                }),
                &["id", "rationale", "ops"],
            ),
        }),
    ]
}
