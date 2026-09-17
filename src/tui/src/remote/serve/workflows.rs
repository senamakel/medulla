//! Answering a client's workflow calls from the host's own store.
//!
//! The Workflows tab on a client that has switched to this host reads *this*
//! machine's workflow store — its catalogue, its runs, its history — and runs
//! workflows here. Each request is one `RemoteMessage::WorkflowCall`, answered
//! by `WorkflowResult`; the work itself is
//! [`medulla::workflows::remote_call::serve_workflow_call`], which reads the
//! store for the `store_*` tools and forwards everything else through the MCP
//! dispatcher so a remote run is admitted by the same policy a local one is.
//!
//! Calls are answered off the serve loop. A `workflow_run` with `wait: "no"`
//! returns as soon as the run is admitted, but a store read on a slow disk or
//! a forwarded tool that validates a graph must not hold up screen frames for
//! every other session on this host.

use std::sync::Arc;

use medulla::protocol::{encode_remote_message, RemoteMessage};

use super::RemoteServer;

/// What the host answers workflow calls with.
#[cfg(feature = "workflows")]
pub struct WorkflowHost {
    /// This machine's workflow store.
    pub store: Arc<dyn medulla::workflows::WorkflowStore>,
    /// The MCP session forwarded tools run under: the host's own policy, the
    /// full authoring surface, no fleet.
    pub session: Arc<medulla::mcp::McpSession>,
}

#[cfg(feature = "workflows")]
impl WorkflowHost {
    /// Build the host's workflow surface from the same loaded config the rest
    /// of the daemon reads, so `workflow_host` answers with this machine's
    /// policy and `workflow_run` starts harnesses the way a local run would.
    pub fn from_config(
        env: &std::collections::HashMap<String, String>,
        loaded: medulla::config::LoadedConfig,
        cwd: &std::path::Path,
    ) -> Self {
        let store: Arc<dyn medulla::workflows::WorkflowStore> =
            medulla::workflows::discover_store(env, cwd);
        let policy = medulla::mcp::policy_from_loaded(loaded);
        let session = Arc::new(medulla::mcp::McpSession::local(
            store.clone(),
            policy,
            medulla::mcp::ToolMode::Full,
        ));
        WorkflowHost { store, session }
    }
}

/// The stand-in for a build without the workflow engine: every call is
/// refused with a reason the client can show.
#[cfg(not(feature = "workflows"))]
pub struct WorkflowHost;

impl RemoteServer {
    /// Serve this host's workflow store to clients.
    ///
    /// Without it every `WorkflowCall` is answered with an error — the client
    /// then says "workflows are not served by this host" rather than waiting.
    pub fn with_workflows(mut self, workflows: WorkflowHost) -> Self {
        self.workflows = Some(Arc::new(workflows));
        self
    }

    /// Answer one call, off the serve loop.
    pub(super) fn handle_workflow_call(
        &self,
        from: &str,
        request_id: String,
        tool: String,
        args: serde_json::Value,
    ) {
        let bridge = self.bridge.clone();
        let to = from.to_string();
        let workflows = self.workflows.clone();
        let log = self.log.clone();
        tokio::spawn(async move {
            let outcome = match workflows {
                #[cfg(feature = "workflows")]
                Some(host) => {
                    medulla::workflows::remote_call::serve_workflow_call(
                        &host.store,
                        &host.session,
                        &tool,
                        args,
                    )
                    .await
                }
                #[cfg(not(feature = "workflows"))]
                Some(_) => Err("this host was built without the workflow engine".to_string()),
                None => Err("this host does not serve workflows".to_string()),
            };
            if let Err(reason) = &outcome {
                if let Some(log) = &log {
                    log(&format!(
                        "remote: workflow call {tool} from {to} failed: {reason}"
                    ));
                }
            }
            let answer = match outcome {
                Ok(result) => RemoteMessage::WorkflowResult {
                    request_id,
                    result: Some(result),
                    error: None,
                },
                Err(error) => RemoteMessage::WorkflowResult {
                    request_id,
                    result: None,
                    error: Some(error),
                },
            };
            if let Err(error) = bridge.send(&to, &encode_remote_message(&answer)).await {
                if let Some(log) = &log {
                    log(&format!("remote: send to {to} failed: {error}"));
                }
            }
        });
    }
}
