//! The Usage subpage: the account's plan and spend, as the backend reports them.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::ui::app::types::App;

impl App {
    /// Draw the Usage subpage: the account's plan, cycle spend, and per-model
    /// breakdown.
    pub(super) fn draw_usage(&mut self, f: &mut Frame, area: Rect) {
        let dim = Style::default().add_modifier(Modifier::DIM);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines: Vec<TLine> = Vec::new();
        lines.push(TLine::from(Span::styled("Account", bold)));
        match &self.account_usage {
            None => lines.push(TLine::from(Span::styled(
                "account usage requires backend login (medulla login) · r to refresh",
                dim,
            ))),
            Some(data) => {
                let g = |path: &[&str]| -> Option<serde_json::Value> {
                    let mut cur = data;
                    for key in path {
                        cur = cur.get(key)?;
                    }
                    Some(cur.clone())
                };
                if let Some(plan) = g(&["plan"]).and_then(|v| v.as_str().map(str::to_string)) {
                    lines.push(TLine::from(format!("plan       {plan}")));
                }
                if let Some(spent) = g(&["inferenceTotals", "spent"]).and_then(|v| v.as_f64()) {
                    let calls = g(&["inferenceTotals", "calls"])
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    lines.push(TLine::from(format!(
                        "cycle      ${spent:.4} spent · {calls} calls"
                    )));
                }
                if let Some(remaining) = g(&["remainingUsd"]).and_then(|v| v.as_f64()) {
                    lines.push(TLine::from(format!("remaining  ${remaining:.4}")));
                }
                if let Some(models) = g(&["inferenceByModel"]).and_then(|v| match v {
                    serde_json::Value::Array(rows) => Some(rows),
                    _ => None,
                }) {
                    for row in models.iter().take(8) {
                        let model = row
                            .get("model")
                            .or_else(|| row.get("_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let spent = row.get("spent").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        lines.push(TLine::from(Span::styled(
                            format!("  {model:<24} ${spent:.4}"),
                            dim,
                        )));
                    }
                }
            }
        }
        lines.push(TLine::from(""));
        lines.push(TLine::from(Span::styled(
            "r refresh · c effective config · 1-9 switch settings pages",
            dim,
        )));
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .block(self.content_panel("Usage")),
            area,
        );
    }
}
