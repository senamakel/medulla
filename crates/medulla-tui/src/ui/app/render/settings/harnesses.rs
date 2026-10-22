//! The Harnesses Settings section and its injected, side-effect-free view data.

use medulla::config::TuiConfig;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// A provider detected by a collector outside the render path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetectedProvider {
    /// Human-facing provider name.
    pub name: String,
    /// Version reported by the collector, when available.
    pub version: Option<String>,
}

/// Facts collected before rendering the Harnesses page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct HarnessesView {
    /// Detected providers and versions, if a provider scan has completed.
    pub providers: Vec<DetectedProvider>,
    /// Whether the provider scan completed, including an empty result.
    pub detection_complete: bool,
    /// Tier-to-model routing rows.
    pub routing: Vec<(String, String)>,
    /// Provider budget descriptions.
    pub budgets: Vec<(String, String)>,
    /// Applied customization descriptions supplied by the caller.
    pub customizations: Vec<String>,
}

impl HarnessesView {
    /// Build configuration facts without probing the host or reading files.
    pub(super) fn from_config(config: &TuiConfig) -> Self {
        let mut view = Self::default();
        if let Some(router) = &config.router {
            view.routing.extend(
                router
                    .models
                    .iter()
                    .map(|(tier, model)| (tier.clone(), model.clone())),
            );
            view.routing.sort();
            if view.routing.is_empty() {
                view.routing.push(("router".into(), "configured".into()));
            }
        }
        if let Some(budget) = &config.budget {
            let mut rows: Vec<_> = budget.providers.iter().collect();
            rows.sort_by(|a, b| a.0.cmp(b.0));
            view.budgets.extend(rows.into_iter().map(|(provider, budget)| {
                let mut details = Vec::new();
                if let Some(seat) = &budget.seat {
                    details.push(format!("seat {seat}"));
                }
                if let Some(used) = budget.used_tokens {
                    details.push(format!("used {used}"));
                }
                if let Some(limit) = budget.limit_tokens {
                    details.push(format!("limit {limit}"));
                }
                if let Some(remaining) = budget.remaining_tokens {
                    details.push(format!("remaining {remaining}"));
                }
                (
                    provider.clone(),
                    if details.is_empty() {
                        "budget configured; values unavailable".into()
                    } else {
                        details.join(" · ")
                    },
                )
            }));
        }
        view.customizations.push(format!(
            "hooks: {} in effective configuration",
            config.hooks.hooks.len()
        ));
        view.customizations.push(format!(
            "Codex overrides: not reported by loaded settings"
        ));
        view.customizations.push(format!(
            "attribution: commits {}",
            if config.attribution.commit {
                "enabled"
            } else {
                "disabled"
            }
        ));
        view.customizations.push(format!(
            "MCP: fleet tools {} · depth {}",
            if config.mcp.fleet_tools {
                "enabled"
            } else {
                "disabled"
            },
            config.mcp.max_depth
        ));
        view.customizations
            .push("skills: attachment state not reported".into());
        view
    }
}

impl super::super::super::types::App {
    /// Render the configuration facts available without running detection.
    pub(super) fn draw_harnesses(&mut self, f: &mut Frame, area: Rect) {
        let view = HarnessesView::from_config(&self.loaded.config);
        draw_harnesses(f, area, &view);
    }
}

/// Draw pre-collected provider and configuration facts without side effects.
pub(super) fn draw_harnesses(f: &mut Frame, area: Rect, view: &HarnessesView) {
    let mut lines = vec![Line::from("Detected providers")];
    if view.providers.is_empty() {
        lines.push(Line::from(if view.detection_complete {
            "No providers detected."
        } else {
            "Provider detection has not been sampled."
        }));
    } else {
        for provider in &view.providers {
            lines.push(Line::from(format!(
                "{} · {}",
                provider.name,
                provider.version.as_deref().unwrap_or("version unavailable")
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Routing"));
    if view.routing.is_empty() {
        lines.push(Line::from("Not configured."));
    } else {
        for (tier, model) in &view.routing {
            lines.push(Line::from(format!("{tier} → {model}")));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Budgets"));
    if view.budgets.is_empty() {
        lines.push(Line::from("No provider budgets configured."));
    } else {
        for (provider, budget) in &view.budgets {
            lines.push(Line::from(format!("{provider} · {budget}")));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Medulla customizations"));
    if view.customizations.is_empty() {
        lines.push(Line::from("No customization facts supplied."));
    } else {
        lines.extend(view.customizations.iter().map(Line::from));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Harnesses")),
        area,
    );
}
