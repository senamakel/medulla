//! The "your free week is up" screen.
//!
//! Drawn when [`medulla::access::decide`] refuses the signed-in account, in
//! place of the app. A pure state machine like [`super::login`]: it renders,
//! takes keys, and reaches an outcome; opening a browser and leaving the process
//! are the caller's jobs.
//!
//! Deliberately not a nag that can be dismissed into the app. The gate exists to
//! say one thing — this account needs a plan — and a screen that could be
//! skipped would say it while meaning nothing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use medulla::access::Denial;

use super::theme::Theme;

/// Where the operator is sent to subscribe.
pub const PRICING_URL: &str = "https://tinyhumans.ai/pricing";

/// What the screen resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeOutcome {
    /// Leave. The only outcome — see the module docs.
    Quit,
}

/// An async action the caller runs on the screen's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeCmd {
    /// Open `url` in the platform browser. Fire-and-forget.
    OpenUrl(String),
}

/// One row of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    /// Open the pricing page. Does not leave the screen.
    Subscribe,
    /// Leave with [`UpgradeOutcome::Quit`].
    Quit,
}

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::Subscribe => "Subscribe",
            Item::Quit => "Quit",
        }
    }
}

const MENU: [Item; 2] = [Item::Subscribe, Item::Quit];

/// The paywall screen's state.
pub struct UpgradeScreen {
    /// The account this refers to, as `describe_me` phrased it.
    who: String,
    /// Why the account was refused, which decides what the panel says.
    denial: Denial,
    menu_index: usize,
    flash: Option<String>,
    outcome: Option<UpgradeOutcome>,
}

impl UpgradeScreen {
    /// A screen for the account described by `who`, refused for `denial`.
    pub fn new(who: impl Into<String>, denial: Denial) -> Self {
        UpgradeScreen {
            who: who.into(),
            denial,
            menu_index: 0,
            flash: None,
            outcome: None,
        }
    }

    /// The headline and the line under it, for this denial.
    ///
    /// [`Denial::Undetermined`] does not mention the trial at all. The operator
    /// may be a paying customer whose plan we simply failed to look up, and
    /// telling them their free week is up would be both wrong and insulting.
    fn message(&self) -> (&'static str, &'static str) {
        match self.denial {
            Denial::TrialExpired => (
                "Your free week is up.",
                "Subscribe to keep using Medulla — plans start at Basic.",
            ),
            Denial::Undetermined => (
                "Your plan could not be confirmed.",
                "If you already subscribe, try again in a few minutes.",
            ),
        }
    }

    /// The outcome, once the screen has reached one.
    pub fn outcome(&self) -> Option<UpgradeOutcome> {
        self.outcome.clone()
    }

    /// Handle one key, optionally emitting a command.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<UpgradeCmd> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.outcome = Some(UpgradeOutcome::Quit);
            return None;
        }
        match key.code {
            KeyCode::Up => {
                self.menu_index = (self.menu_index + MENU.len() - 1) % MENU.len();
                None
            }
            KeyCode::Down => {
                self.menu_index = (self.menu_index + 1) % MENU.len();
                None
            }
            // Esc leaves. There is nothing to back out to: the app behind this
            // screen is the thing the account may not use.
            KeyCode::Esc => {
                self.outcome = Some(UpgradeOutcome::Quit);
                None
            }
            KeyCode::Enter => match MENU[self.menu_index.min(MENU.len() - 1)] {
                Item::Subscribe => {
                    self.flash = Some(format!("opened {PRICING_URL}"));
                    Some(UpgradeCmd::OpenUrl(PRICING_URL.to_string()))
                }
                Item::Quit => {
                    self.outcome = Some(UpgradeOutcome::Quit);
                    None
                }
            },
            _ => None,
        }
    }

    /// Render the centered panel.
    pub fn draw(&mut self, f: &mut Frame) {
        let theme = Theme::default();
        let mut lines: Vec<Line> = Vec::new();
        for row in super::LOGO {
            lines.push(Line::from(Span::styled(
                row,
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            self.who.clone(),
            Style::default().add_modifier(Modifier::DIM),
        )));
        lines.push(Line::from(""));
        let (headline, detail) = self.message();
        lines.push(Line::from(headline));
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {PRICING_URL}"),
            Style::default().fg(Color::Blue),
        )));
        lines.push(Line::from(""));

        for (i, item) in MENU.iter().enumerate() {
            let selected = i == self.menu_index;
            let style = if selected {
                Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{} {:<32}", if selected { "▸" } else { " " }, item.label()),
                style,
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑↓ choose · Enter select · Esc quit",
            Style::default().add_modifier(Modifier::DIM),
        )));

        if let Some(flash) = &self.flash {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                flash.clone(),
                Style::default().fg(Color::Green),
            )));
        }

        let screen = f.area();
        let width = super::layout::panel_width(&lines, screen.width);
        let height = super::layout::panel_height(&lines, width, screen.height);
        let area = super::layout::centered_fixed(width, height, screen);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.primary))
            .title(Span::styled(
                " subscribe ",
                Style::default().add_modifier(Modifier::DIM),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;
