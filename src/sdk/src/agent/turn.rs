//! Driving one local agent turn.
//!
//! # Why this does not fold the event stream
//!
//! It used to, and that was the wrong seam. Medulla already has one vocabulary
//! for what a turn is doing — [`crate::protocol::HarnessEventKind`], which every
//! spawned CLI harness maps into so the run view renders any of them without
//! knowing which provider ran. A fold here would have been a *second* one, and
//! the consumer would then have had to translate between them.
//!
//! So this forwards [`AgentEvent`] verbatim and lets the consumer map it. The
//! mapping for the daemon provider lives beside its sibling line-mappers, in
//! [`crate::daemon::providers::local`], which is where a reader comparing the
//! embedded harness with a spawned one would look.

use std::sync::Arc;

use tinyagents::harness::events::{AgentEvent, EventListener, EventRecord, EventSink};
use tinyagents::harness::message::{AssistantMessage, Message};

use super::harness::{self, Limits, Route};
use super::tools::Workspace;

/// What a completed turn produced.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// The final assistant text.
    pub reply: String,
    /// Input and output tokens, when the provider reported them.
    pub usage: Option<(u64, u64)>,
}

/// Forwards every harness event to the caller's sink.
///
/// A listener rather than a post-hoc walk of the finished run: the rows must
/// reach the run view *while* the turn is producing, and a transcript assembled
/// at the end is one nobody watched. It is also what makes the caller's idle
/// watchdog an idle watchdog rather than a stopwatch — a turn emitting deltas
/// is alive even when it has not finished.
struct Forwarder {
    sink: Arc<dyn Fn(AgentEvent) + Send + Sync>,
}

impl EventListener for Forwarder {
    fn on_event(&self, record: &EventRecord) {
        (self.sink)(record.event.clone());
    }
}

/// Run one turn against `route` inside `workspace`.
///
/// `on_event` receives every [`AgentEvent`] the harness emits, as it is emitted.
///
/// # Errors
///
/// Returns a sentence when the harness itself fails — an unreachable endpoint, a
/// rejected credential, a limit tripped before any answer. A failing *tool* is
/// not one of these: it is reported as an event and the model gets to react.
pub async fn run(
    prompt: &str,
    route: &Route,
    workspace: Workspace,
    limits: &Limits,
    thread_id: &str,
    on_event: Arc<dyn Fn(AgentEvent) + Send + Sync>,
) -> Result<TurnOutcome, String> {
    let agent = harness::build(route, workspace.clone(), limits);

    let sink = EventSink::with_stream_id(thread_id);
    sink.subscribe(Arc::new(Forwarder { sink: on_event }));

    let config = tinyagents::harness::context::RunConfig::new(thread_id)
        .with_thread(thread_id)
        .with_max_model_calls(limits.max_model_calls)
        .with_max_tool_calls(limits.max_tool_calls);
    let ctx = tinyagents::harness::context::RunContext::new(config, ()).with_events(sink);

    let input = vec![
        Message::system(harness::system_prompt(&workspace)),
        Message::user(prompt),
    ];

    // Streaming rather than unary: the deltas are what let a consumer show a
    // turn working, and what let its watchdog tell working from idle.
    let run = agent
        .invoke_streaming_in_context(&(), ctx, input)
        .await
        .map_err(|e| format!("the agent turn failed: {e}"))?;

    let reply = run
        .final_response
        .as_ref()
        .map(|r| visible_text(&r.message))
        .unwrap_or_default();

    Ok(TurnOutcome {
        reply,
        usage: Some((run.usage.usage.input_tokens, run.usage.usage.output_tokens)),
    })
}

/// The assistant text an operator should see.
///
/// `ContentBlock::as_text` returns `None` for `Thinking`, which is the point:
/// reasoning is preserved on the message so a provider that requires verbatim
/// replay can round-trip it, but rendering it as the reply would put the model's
/// scratch work in front of the operator as though it were the answer.
fn visible_text(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .collect::<Vec<_>>()
        .join("")
}
