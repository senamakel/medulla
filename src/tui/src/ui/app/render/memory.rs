//! The Memory tab's overview, search, and maintenance pages.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TLine, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::ui::multi_pane;
use crate::ui::util::clip;
use medulla::memory::MemoryStatus;

use super::super::types::{
    App, MemoryEntry, MEMORY_SUBPAGES, MP_MAINTENANCE, MP_OVERVIEW, MP_SEARCH,
};

impl App {
    /// Draw the Memory menu and its active content page.
    pub(super) fn draw_memory(&mut self, f: &mut Frame, area: Rect) {
        let (nav, content) = multi_pane::split(area);
        multi_pane::draw_nav(
            f,
            nav,
            self.panel("Memory"),
            &self.theme,
            &MEMORY_SUBPAGES,
            &[],
            self.memory_subpage_index,
            self.memory_focused,
        );
        match self.memory_subpage_index {
            MP_OVERVIEW => self.draw_memory_overview(f, content),
            MP_SEARCH => self.draw_memory_search(f, content),
            MP_MAINTENANCE => self.draw_memory_maintenance(f, content),
            _ => self.draw_memory_overview(f, content),
        }
    }

    /// Draw persona status plus the distilled directives and facet summaries.
    fn draw_memory_overview(&mut self, f: &mut Frame, area: Rect) {
        // Disabled / not wired: a single helpful hint panel.
        let enabled = self
            .memory_status
            .as_ref()
            .map(|s| s.enabled)
            .unwrap_or(false);
        if !enabled {
            let mut lines = vec![TLine::from(Span::styled(
                "Persona memory is not enabled.",
                Style::default().fg(Color::Yellow),
            ))];
            lines.push(TLine::from(Span::styled(
                "Enable it in config (memory.enabled = true) with an OpenRouter key,",
                Style::default().add_modifier(Modifier::DIM),
            )));
            lines.push(TLine::from(Span::styled(
                "then run `medulla memory backfill` to distil your persona pack.",
                Style::default().add_modifier(Modifier::DIM),
            )));
            f.render_widget(
                Paragraph::new(Text::from(lines))
                    .wrap(Wrap { trim: true })
                    .block(self.panel("Persona memory")),
                area,
            );
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(0)])
            .split(area);

        // Status header.
        let st = self.memory_status.clone().unwrap_or(MemoryStatus {
            enabled: true,
            workspace: String::new(),
            pack_exists: false,
            pack_path: String::new(),
            entry_count: 0,
            directives_count: 0,
            facet_counts: Default::default(),
        });
        let mut header = vec![
            TLine::from(vec![
                Span::styled("● enabled", Style::default().fg(Color::Green)),
                Span::raw(format!(" · {}", clip(&st.workspace, 48))),
            ]),
            if st.pack_exists {
                TLine::from(Span::styled(
                    format!("pack ● present · {}", clip(&st.pack_path, 52)),
                    Style::default().fg(Color::Green),
                ))
            } else {
                TLine::from(Span::styled(
                    "pack ○ absent · press b to backfill",
                    Style::default().add_modifier(Modifier::DIM),
                ))
            },
            TLine::from(format!(
                "{} observation(s) · {} directive(s)",
                st.entry_count, st.directives_count
            )),
            TLine::from(Span::styled(
                "r refresh · maintenance controls live on page 3",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        let facets = if st.facet_counts.is_empty() {
            "facets: (none)".to_string()
        } else {
            let joined = st
                .facet_counts
                .iter()
                .map(|(f, n)| format!("{f}={n}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("facets: {joined}")
        };
        header.push(TLine::from(Span::styled(
            facets,
            Style::default().fg(self.theme.primary),
        )));
        f.render_widget(
            Paragraph::new(Text::from(header))
                .wrap(Wrap { trim: true })
                .block(self.panel("Persona memory")),
            rows[0],
        );

        // Left list + right detail.
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(rows[1]);

        let entries = self.memory_overview_entries();
        let idx = self.memory_index.min(entries.len().saturating_sub(1));
        let block = self.panel("Directives & facets");
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);
        let vis = (inner.height as usize).max(1);
        let start = idx
            .saturating_sub(vis / 2)
            .min(entries.len().saturating_sub(vis));
        let mut lines: Vec<TLine> = Vec::new();
        for (i, entry) in entries.iter().enumerate().skip(start).take(vis) {
            let (label, base) = match entry {
                MemoryEntry::Directive(text) => (
                    format!("◆ {}", clip(text, 30)),
                    Style::default().fg(Color::Yellow),
                ),
                MemoryEntry::Facet { name, count } => (
                    format!("▪ {name} · {count}"),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                MemoryEntry::Hit(hit) => (
                    format!("{} · {} · {:.2}", hit.facet, hit.tier, hit.score),
                    Style::default().fg(Color::Magenta),
                ),
            };
            let mut style = base;
            if i == idx {
                style = self.theme.selection();
            }
            lines.push(TLine::from(Span::styled(label, style)));
        }
        if entries.is_empty() {
            lines.push(TLine::from(Span::styled(
                "No directives or observations yet. Open Maintenance to backfill.",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        f.render_widget(Paragraph::new(Text::from(lines)), inner);

        // Detail pane.
        let (title, body) = self.memory_detail(entries.get(idx));
        f.render_widget(
            Paragraph::new(Text::from(body))
                .wrap(Wrap { trim: false })
                .block(self.panel(title)),
            cols[1],
        );
    }

    /// Draw ranked memory-search results and an explicit query affordance.
    fn draw_memory_search(&mut self, f: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(area);
        let query = self.memory_query.as_deref().unwrap_or("(none yet)");
        let header = vec![
            TLine::from(format!("query: {}", clip(query, 64))),
            TLine::from(Span::styled(
                "Enter, /, or q opens a new search · ↑↓ browse results",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        f.render_widget(
            Paragraph::new(Text::from(header))
                .wrap(Wrap { trim: true })
                .block(self.panel("Search memory")),
            rows[0],
        );

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(rows[1]);
        let entries: Vec<MemoryEntry> = self
            .memory_hits
            .iter()
            .cloned()
            .map(MemoryEntry::Hit)
            .collect();
        let idx = self.memory_index.min(entries.len().saturating_sub(1));
        let block = self.panel(format!("Results · {} hit(s)", entries.len()));
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);
        let mut lines = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let MemoryEntry::Hit(hit) = entry else {
                continue;
            };
            let style = if index == idx {
                self.theme.selection()
            } else {
                Style::default().fg(Color::Magenta)
            };
            lines.push(TLine::from(Span::styled(
                format!("{} · {} · {:.2}", hit.facet, hit.tier, hit.score),
                style,
            )));
        }
        if entries.is_empty() {
            lines.push(TLine::from(Span::styled(
                if self.memory_query.is_some() {
                    "No hits for that query."
                } else {
                    "Run a search to rank observations across facets."
                },
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
        let (title, body) = self.memory_detail(entries.get(idx));
        f.render_widget(
            Paragraph::new(Text::from(body))
                .wrap(Wrap { trim: false })
                .block(self.panel(title)),
            cols[1],
        );
    }

    /// Draw memory workspace state and the paid ingest controls in one place.
    fn draw_memory_maintenance(&mut self, f: &mut Frame, area: Rect) {
        let Some(status) = self.memory_status.as_ref() else {
            self.draw_memory_disabled(f, area);
            return;
        };
        let state = if status.enabled {
            Span::styled("● enabled", Style::default().fg(Color::Green))
        } else {
            Span::styled("○ disabled", Style::default().fg(Color::Yellow))
        };
        let progress = if self.memory_ingesting {
            TLine::from(Span::styled(
                "● ingesting… this can take a while",
                Style::default().fg(Color::Yellow),
            ))
        } else {
            TLine::from(Span::styled(
                "b backfill everything · i ingest new activity · r refresh",
                Style::default().add_modifier(Modifier::DIM),
            ))
        };
        let lines = vec![
            TLine::from(state),
            TLine::from(format!("workspace: {}", status.workspace)),
            TLine::from(format!(
                "persona pack: {}",
                if status.pack_exists {
                    status.pack_path.as_str()
                } else {
                    "absent"
                }
            )),
            TLine::from(format!(
                "{} observation(s) · {} directive(s)",
                status.entry_count, status.directives_count
            )),
            TLine::from(""),
            progress,
            TLine::from(Span::styled(
                "Backfill and ingest may call a paid provider; duplicate runs are blocked.",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .block(self.panel("Maintenance")),
            area,
        );
    }

    /// Explain how to enable persona memory when no status is available.
    fn draw_memory_disabled(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            TLine::from(Span::styled(
                "Persona memory is not enabled.",
                Style::default().fg(Color::Yellow),
            )),
            TLine::from(Span::styled(
                "Enable memory.enabled in config and provide an OpenRouter key.",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: true })
                .block(self.panel("Persona memory")),
            area,
        );
    }

    /// The detail title + wrapped body for the selected Memory entry.
    pub(super) fn memory_detail(
        &self,
        entry: Option<&MemoryEntry>,
    ) -> (String, Vec<TLine<'static>>) {
        let dim = Style::default().add_modifier(Modifier::DIM);
        match entry {
            None => (
                "Detail".into(),
                vec![TLine::from(Span::styled(
                    "Select an entry with ↑/↓ (or search with /memory <query>).",
                    dim,
                ))],
            ),
            Some(MemoryEntry::Directive(text)) => {
                ("Directive".into(), vec![TLine::from(text.clone())])
            }
            Some(MemoryEntry::Facet { name, count }) => (
                name.clone(),
                vec![
                    TLine::from(format!("{count} observation(s) in this facet.")),
                    TLine::from(Span::styled(
                        "Run /memory <query> to rank observations across facets.",
                        dim,
                    )),
                ],
            ),
            Some(MemoryEntry::Hit(hit)) => {
                let mut body = vec![TLine::from(hit.text.clone()), TLine::from("")];
                if let Some(q) = &hit.quote {
                    body.push(TLine::from(Span::styled(format!("“{q}”"), dim)));
                    body.push(TLine::from(""));
                }
                body.push(TLine::from(Span::styled(
                    format!(
                        "facet {} · tier {} · score {:.3}",
                        hit.facet, hit.tier, hit.score
                    ),
                    dim,
                )));
                body.push(TLine::from(Span::styled(hit.timestamp.clone(), dim)));
                (format!("{} · {}", hit.facet, hit.tier), body)
            }
        }
    }
}
