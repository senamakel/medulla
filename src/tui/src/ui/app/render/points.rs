//! The dummy TokenMaxxing experience: season progress, daily bounties, and the
//! current daily leaderboard.
//!
//! Every value is intentionally local sample data. This keeps the design
//! reviewable before the eventual rewards service defines persistence,
//! eligibility, payout, and reset semantics.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::super::types::App;

/// One deterministic daily challenge rendered in the bounty strip.
struct DailyBounty {
    title: &'static str,
    detail: &'static str,
    reward: &'static str,
    progress: u8,
    target: u8,
}

/// Dummy bounties used while the program rules and backend contract are shaped.
const DAILY_BOUNTIES: [DailyBounty; 3] = [
    DailyBounty {
        title: "Ship something useful",
        detail: "Complete one agent task",
        reward: "+120 pts",
        progress: 1,
        target: 1,
    },
    DailyBounty {
        title: "Close the loop",
        detail: "Finish three assigned tasks",
        reward: "+80 pts",
        progress: 2,
        target: 3,
    },
    DailyBounty {
        title: "Explorer bonus",
        detail: "Try a new workflow",
        reward: "+200 pts",
        progress: 0,
        target: 1,
    },
];

/// One deterministic competitor rendered in the daily standings.
struct LeaderboardEntry {
    rank: &'static str,
    name: &'static str,
    today: &'static str,
    streak: &'static str,
    total: &'static str,
    prize: &'static str,
    is_you: bool,
}

/// Dummy daily standings, including the current user outside the leading pack.
const LEADERBOARD: [LeaderboardEntry; 7] = [
    LeaderboardEntry {
        rank: "1",
        name: "mira.dev",
        today: "640",
        streak: "14d",
        total: "12,840",
        prize: "1,000 pts + $25",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "2",
        name: "byteforge",
        today: "590",
        streak: "9d",
        total: "11,960",
        prize: "500 pts",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "3",
        name: "luna_ops",
        today: "520",
        streak: "21d",
        total: "11,420",
        prize: "250 pts",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "4",
        name: "agent_olive",
        today: "475",
        streak: "6d",
        total: "9,870",
        prize: "—",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "5",
        name: "niko.builds",
        today: "430",
        streak: "4d",
        total: "8,940",
        prize: "—",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "⋮",
        name: "",
        today: "",
        streak: "",
        total: "",
        prize: "",
        is_you: false,
    },
    LeaderboardEntry {
        rank: "12",
        name: "You",
        today: "340",
        streak: "7d",
        total: "2,480",
        prize: "↑ 3 places",
        is_you: true,
    },
];

impl App {
    /// Draw the offline-first TokenMaxxing program concept.
    pub(super) fn draw_points(&mut self, f: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(if area.width >= 90 { 7 } else { 5 }),
                Constraint::Min(9),
            ])
            .split(area);

        for pane in rows.iter() {
            self.note_pane(*pane);
        }
        self.draw_points_summary(f, rows[0]);
        self.draw_daily_bounties(f, rows[1]);
        self.draw_leaderboard(f, rows[2]);
    }

    /// Draw the current user's season position and next-level progress.
    fn draw_points_summary(&self, f: &mut Frame, area: Rect) {
        let progress_width = area.width.saturating_sub(39).clamp(8, 34) as usize;
        let filled = progress_width.saturating_mul(2480) / 3000;
        let bar = format!(
            "{}{}",
            "━".repeat(filled),
            "─".repeat(progress_width.saturating_sub(filled))
        );
        let body = Text::from(vec![
            Line::from(vec![
                Span::styled(
                    "2,480 pts",
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  ·  Level 7 Builder  ·  #12 today",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(bar, Style::default().fg(self.theme.accent)),
                Span::styled(
                    "  520 pts to Level 8",
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ]),
            Line::from(Span::styled(
                "7 day streak  🔥  ·  best 14 days  ·  top 8% this season",
                Style::default().fg(Color::Yellow),
            )),
        ]);
        f.render_widget(
            Paragraph::new(body).block(self.panel("TokenMaxxing · Season 01 · 18 days left")),
            area,
        );
    }

    /// Draw three daily earning opportunities, collapsing to a compact list on
    /// terminals too narrow to give each bounty its own card.
    fn draw_daily_bounties(&self, f: &mut Frame, area: Rect) {
        let block = self.panel("Daily bounties · 1/3 claimed · refreshes in 08h 42m");
        let inner = block.inner(area);
        f.render_widget(block, area);

        if area.width < 90 {
            let lines = DAILY_BOUNTIES
                .iter()
                .map(|bounty| {
                    let marker = if bounty.progress == bounty.target {
                        "✓"
                    } else {
                        "○"
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("{marker} {:<22}", bounty.title),
                            Style::default().fg(if bounty.progress == bounty.target {
                                Color::Green
                            } else {
                                self.theme.primary
                            }),
                        ),
                        Span::styled(
                            format!(" {}/{}  {}", bounty.progress, bounty.target, bounty.reward),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
            return;
        }

        let cards = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 3); 3])
            .split(inner);
        for (index, bounty) in DAILY_BOUNTIES.iter().enumerate() {
            let complete = bounty.progress == bounty.target;
            let marker = if complete {
                "✓ COMPLETE"
            } else {
                "○ IN PROGRESS"
            };
            let color = if complete {
                Color::Green
            } else {
                self.theme.primary
            };
            let card = Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(self.theme.dim_border));
            let body = Text::from(vec![
                Line::from(Span::styled(
                    marker,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    bounty.title,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    bounty.detail,
                    Style::default().add_modifier(Modifier::DIM),
                )),
                Line::from(vec![
                    Span::styled(bounty.reward, Style::default().fg(self.theme.accent)),
                    Span::raw(format!("  {}/{}", bounty.progress, bounty.target)),
                ]),
            ]);
            f.render_widget(Paragraph::new(body).block(card), cards[index]);
        }
    }

    /// Draw today's ranked competition and the daily winner bounty.
    fn draw_leaderboard(&self, f: &mut Frame, area: Rect) {
        let narrow = area.width < 82;
        let mut lines = vec![if narrow {
            Line::from(vec![
                Span::styled(
                    format!("{:<4}{:<18}", "#", "PLAYER"),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                Span::styled(
                    "TODAY     TOTAL",
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ])
        } else {
            Line::from(vec![Span::styled(
                format!(
                    "{:<5}{:<22}{:>8}{:>10}{:>12}   {}",
                    "#", "PLAYER", "TODAY", "STREAK", "TOTAL", "DAILY PRIZE"
                ),
                Style::default().add_modifier(Modifier::DIM),
            )])
        }];

        for entry in LEADERBOARD.iter() {
            let row = if narrow {
                format!(
                    "{:<4}{:<18}{:>5}  {:>8}",
                    entry.rank, entry.name, entry.today, entry.total
                )
            } else {
                format!(
                    "{:<5}{:<22}{:>8}{:>10}{:>12}   {}",
                    entry.rank, entry.name, entry.today, entry.streak, entry.total, entry.prize
                )
            };
            let style = if entry.is_you {
                self.theme.selection()
            } else if matches!(entry.rank, "1" | "2" | "3") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(row, style)));
        }

        let title = Line::from(vec![
            Span::styled(
                "Daily leaderboard",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " · closes at 00:00 UTC · prize pool 1,750 pts + $25",
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);
        let block = self.panel("").title(title);
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .alignment(Alignment::Left)
                .block(block),
            area,
        );
    }
}
