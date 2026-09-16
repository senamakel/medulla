//! Answer a client's `WorkflowCall` against this host's own workflow store.
//!
//! The remote-session protocol carries one message for every workflow
//! operation a client wants done *here* rather than on its own machine
//! (`RemoteMessage::WorkflowCall`). Two families of tool arrive on it:
//!
//! - `store_*` reads answer with the store's own record types, verbatim. The
//!   client's canvas draws a `WorkflowRecord` and its rail a `RunRecord`, and
//!   the MCP tools project both into summaries a model reads — so the client
//!   would otherwise have to reconstruct what it needs from a lossy view.
//! - every other name is a `workflow_*` MCP tool, forwarded through
//!   [`crate::mcp::call_tool`] so a run or a delete started from across the
//!   network is admitted by exactly the policy a harness's call would be.
//!
//! Nothing here knows about the transport: the daemon parses the message,
//! calls [`serve_workflow_call`], and sends whatever comes back.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::mcp::McpSession;
use crate::workflows::WorkflowStore;

/// The `store_*` tool names, so a client can tell a store read from a
/// forwarded MCP call without guessing.
pub const STORE_TOOLS: [&str; 7] = [
    "store_list",
    "store_get",
    "store_list_runs",
    "store_get_run",
    "store_list_revisions",
    "store_list_notes",
    "store_list_proposals",
];

/// Run one workflow call and return the tool's JSON.
///
/// # Errors
///
/// The operator-facing reason, when the store read failed, the record does not
/// exist, the argument is missing, or the forwarded MCP tool reported an error.
pub async fn serve_workflow_call(
    store: &Arc<dyn WorkflowStore>,
    session: &McpSession,
    tool: &str,
    args: Value,
) -> Result<Value, String> {
    match tool {
        "store_list" => to_value(store.list().map_err(|e| e.to_string())?),
        "store_get" => {
            let id = string_arg(&args, "id")?;
            let record = store
                .get(&id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no workflow called {id}"))?;
            to_value(record)
        }
        "store_list_runs" => {
            let id = string_arg(&args, "id")?;
            to_value(store.list_runs(&id).map_err(|e| e.to_string())?)
        }
        "store_get_run" => {
            let run_id = string_arg(&args, "runId")?;
            let run = store
                .get_run(&run_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no run called {run_id}"))?;
            to_value(run)
        }
        "store_list_revisions" => {
            let id = string_arg(&args, "id")?;
            to_value(store.list_revisions(&id).map_err(|e| e.to_string())?)
        }
        "store_list_notes" => {
            let id = string_arg(&args, "id")?;
            to_value(store.list_notes(&id).map_err(|e| e.to_string())?)
        }
        "store_list_proposals" => {
            let id = string_arg(&args, "id")?;
            to_value(store.list_proposals(&id).map_err(|e| e.to_string())?)
        }
        other => crate::mcp::call_tool(session, other, args).await,
    }
}

/// Serialize a store record for the wire.
fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| format!("could not encode the answer: {e}"))
}

/// A required string argument, trimmed and non-empty.
fn string_arg(args: &Value, name: &str) -> Result<String, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing required argument '{name}'"))
}

#[cfg(test)]
#[path = "remote_call_tests.rs"]
mod tests;
