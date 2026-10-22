//! Pure derivations over a [`RuntimeSnapshot`](crate::runtime::RuntimeSnapshot)'s
//! event and thread streams.
//!
//! These functions fold the flat event log (and the thread tree) into the small
//! numeric summaries a front end renders — token usage, in-flight call counts,
//! the most recently observed model per tier, and thread nesting depth. They are
//! stateless and side-effect free, so any front end (TUI, CLI, tests) can reuse
//! them.

#[cfg(test)]
mod tests;

use medulla_tokens::Usage;
pub use medulla_tokens::{TierUsage, UsageFold};

use crate::ui::events::{EventEnvelope, TuiEvent};

/// Token accounting grouped by inference tier, reported provider, and model.
///
/// Provider is intentionally not inferred from a tier or model name. Event
/// streams that do not carry provider attribution leave it to the consuming
/// view to display as unknown.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedUsage {
    /// Model tier selected by orchestration.
    pub tier: String,
    /// Provider attribution, when explicitly reported by the event source.
    pub provider: Option<crate::protocol::HarnessProvider>,
    /// Concrete model identifier, when reported.
    pub model: Option<String>,
    /// Accumulated token categories for this tier/model pair.
    pub usage: Usage,
}

/// Fold inference usage into category-preserving tier/model rows.
///
/// Optional categories remain absent when no event reports them. Once a
/// category is reported, later events without that category do not erase it.
pub fn detailed_usage(events: &[EventEnvelope]) -> Vec<DetailedUsage> {
    let mut rows: std::collections::BTreeMap<
        (String, Option<crate::protocol::HarnessProvider>, Option<String>),
        Usage,
    > = std::collections::BTreeMap::new();
    for envelope in events {
        let TuiEvent::InferenceEnd {
            tier,
            provider,
            model,
            usage: Some(usage),
            ..
        } = &envelope.event
        else {
            continue;
        };
        let total = rows
            .entry((tier.clone(), *provider, model.clone()))
            .or_default();
        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        add_optional(&mut total.cache_read_tokens, usage.cache_read_tokens);
        add_optional(
            &mut total.cache_creation_tokens,
            usage.cache_creation_tokens,
        );
        add_optional(&mut total.cache_write_tokens, usage.cache_write_tokens);
        add_optional(&mut total.reasoning_tokens, usage.reasoning_tokens);
    }
    rows.into_iter()
        .map(|((tier, provider, model), usage)| DetailedUsage {
            tier,
            provider,
            model,
            usage,
        })
        .collect()
}

/// Estimate provider usage cost through the shared pricing snapshot.
///
/// `None` means provider/model pricing is unavailable or the report includes a
/// category with no published price.
pub fn usage_cost(
    provider: crate::protocol::HarnessProvider,
    model: &str,
    usage: &Usage,
) -> Option<f64> {
    medulla_tokens::cost(provider, model, usage).map(|cost| cost.usd)
}

fn add_optional(total: &mut Option<i64>, value: Option<i64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or_default() + value);
    }
}

/// Fold the event log into per-tier and per-task token totals for the Usage tab.
///
/// `InferenceStart` increments a tier's call count; `InferenceEnd` adds its
/// reported input/output tokens; `TaskComplete` accumulates the delegated
/// sub-agent totals and appends a per-task row. Events without usage are ignored.
pub fn usage_fold(events: &[EventEnvelope]) -> UsageFold {
    let mut fold = UsageFold::default();
    for env in events {
        match &env.event {
            TuiEvent::InferenceStart { tier, .. } => {
                fold.tier_mut(tier).calls += 1;
            }
            TuiEvent::InferenceEnd {
                tier,
                usage: Some(u),
                ..
            } => {
                let t = fold.tier_mut(tier);
                t.input_tokens += u.input_tokens;
                t.output_tokens += u.output_tokens;
            }
            TuiEvent::TaskComplete { digest } => {
                if let Some(u) = &digest.usage {
                    fold.subagent.input_tokens += u.input_tokens;
                    fold.subagent.output_tokens += u.output_tokens;
                    fold.subagent.calls += 1;
                    fold.tasks
                        .push((digest.task_id.clone(), u.input_tokens, u.output_tokens));
                }
            }
            _ => {}
        }
    }
    fold
}

/// Count inference calls currently in flight by folding start/end deltas over the
/// event log, clamped at zero so a stray `InferenceEnd` never drives it negative.
pub fn running_calls(events: &[EventEnvelope]) -> i64 {
    let mut n = 0i64;
    for e in events {
        match e.event {
            TuiEvent::InferenceStart { .. } => n += 1,
            TuiEvent::InferenceEnd { .. } => n = (n - 1).max(0),
            _ => {}
        }
    }
    n
}

/// The most recent model id observed on the stream for `tier`, if any.
///
/// Scans the log newest-first and returns the `model` of the first
/// `InferenceStart`/`InferenceEnd` matching the tier that carries one.
pub fn observed_model<'a>(events: &'a [EventEnvelope], tier: &str) -> Option<&'a str> {
    events.iter().rev().find_map(|e| match &e.event {
        TuiEvent::InferenceStart { tier: t, model, .. }
        | TuiEvent::InferenceEnd { tier: t, model, .. }
            if t == tier =>
        {
            model.as_deref()
        }
        _ => None,
    })
}
