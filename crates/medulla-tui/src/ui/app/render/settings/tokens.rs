//! The Tokens Settings section and its injected, side-effect-free view data.

use medulla::subscriptions::{MeterId, SubscriptionSnapshot};
use medulla_tokens::{cost, Usage};
use medulla_types::HarnessProvider;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// One accumulated session/tier/model usage row supplied to the renderer.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TokenUsageRow {
    /// Session label. Runtime-only rows use `current session`.
    pub session: String,
    /// Orchestration tier.
    pub tier: String,
    /// Provider identity, when the source reported it.
    pub provider: Option<HarnessProvider>,
    /// Concrete provider model identifier, when the source reported it.
    pub model: Option<String>,
    /// Token totals, retaining optional categories as optional.
    pub usage: Usage,
}

/// Everything the page can display, detached from discovery and sampling.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct TokensView {
    /// Per-session and per-provider token usage rows.
    pub rows: Vec<TokenUsageRow>,
    /// Latest subscription readings supplied by the app.
    pub subscriptions: SubscriptionSnapshot,
}

/// Draw token accounting and subscription readings without probing providers.
pub(super) fn draw_tokens(f: &mut Frame, area: Rect, view: &TokensView) {
    let mut lines = vec![Line::from(Span::raw("Token usage"))];
    if view.rows.is_empty() {
        lines.push(Line::from("No inference usage reported yet."));
    }
    for row in &view.rows {
        let provider = row
            .provider
            .map(|provider| provider.as_str())
            .unwrap_or("unknown provider");
        let model = row.model.as_deref().unwrap_or("unknown model");
        lines.push(Line::from(format!(
            "{} · {} · {provider} · {model}",
            row.session, row.tier
        )));
        lines.push(Line::from(format!(
            "  input {} · output {}",
            row.usage.input_tokens, row.usage.output_tokens
        )));
        lines.push(Line::from(format!(
            "  cache read {} · cache write {} · reasoning {}",
            value(row.usage.cache_read_tokens),
            value(row.usage.cache_write_tokens.or(row.usage.cache_creation_tokens)),
            value(row.usage.reasoning_tokens)
        )));
        let estimate = row
            .provider
            .zip(row.model.as_deref())
            .and_then(|(provider, model)| cost(provider, model, &row.usage));
        match estimate {
            Some(estimate) => lines.push(Line::from(format!("  cost ${:.4}", estimate.usd))),
            None => lines.push(Line::from("  cost unavailable")),
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Subscription meters"));
    let mut shown_meter = false;
    for meter in MeterId::ALL {
        let Some(reading) = view.subscriptions.get(meter) else {
            continue;
        };
        shown_meter = true;
        let detail = if let Some(fraction) = reading.used_fraction {
            format!("{:.0}% used", fraction.clamp(0.0, 1.0) * 100.0)
        } else if let Some(remaining) = reading.remaining {
            let currency = reading.currency.as_deref().unwrap_or("USD");
            format!("{currency} {remaining:.2} remaining")
        } else if let Some(note) = &reading.note {
            note.clone()
        } else {
            "no value reported".into()
        };
        lines.push(Line::from(format!("{} {detail}", meter.label())));
    }
    if !shown_meter {
        lines.push(Line::from("No subscription readings supplied."));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Tokens")),
        area,
    );
}

fn value(value: Option<i64>) -> String {
    value.map_or_else(|| "unknown".into(), |value| value.to_string())
}
