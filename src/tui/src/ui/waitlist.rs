//! Authenticated Medulla waitlist gate shown before the hosted runtime starts.

use std::io::Stdout;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Terminal;

use medulla::auth::open_browser;
use medulla::client::{MedullaClient, WaitlistState, WaitlistStatus};

/// How the startup loop should continue after the waitlist screen exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitlistOutcome {
    /// Server-side access was granted.
    Approved,
    /// The user requested the existing Claude/Codex history upload flow.
    PowerUser,
    /// The user quit without access.
    Quit,
}

/// Drive the waitlist screen, polling every 30 seconds and accepting invite codes.
pub async fn run_waitlist_ui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    client: &MedullaClient,
) -> anyhow::Result<WaitlistOutcome> {
    let mut status = client.waitlist_status().await?;
    let mut invite = String::new();
    let mut entering_invite = false;
    let mut message: Option<String> = None;
    let mut events = EventStream::new();
    let mut poll = tokio::time::interval(Duration::from_secs(30));
    poll.tick().await;

    loop {
        if status.has_medulla_access || status.status == WaitlistState::Approved {
            return Ok(WaitlistOutcome::Approved);
        }
        terminal
            .draw(|frame| draw(frame, &status, &invite, entering_invite, message.as_deref()))?;

        tokio::select! {
            maybe_event = events.next() => {
                let Some(Ok(Event::Key(key))) = maybe_event else { continue };
                if key.kind == KeyEventKind::Release { continue; }
                if entering_invite {
                    match key.code {
                        KeyCode::Esc => { entering_invite = false; invite.clear(); }
                        KeyCode::Backspace => { invite.pop(); }
                        KeyCode::Char(c) => invite.push(c),
                        KeyCode::Enter if !invite.trim().is_empty() => {
                            match client.redeem_medulla_invite(invite.clone()).await {
                                Ok(next) => { status = next; message = Some("invite accepted".into()); }
                                Err(error) => message = Some(error.to_string()),
                            }
                            entering_invite = false;
                            invite.clear();
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => return Ok(WaitlistOutcome::Quit),
                    KeyCode::Char('i') => entering_invite = true,
                    KeyCode::Char('p') => return Ok(WaitlistOutcome::PowerUser),
                    KeyCode::Char('r') => match client.waitlist_status().await {
                        Ok(next) => { status = next; message = Some("position refreshed".into()); }
                        Err(error) => message = Some(error.to_string()),
                    },
                    KeyCode::Char('g') => {
                        open_browser("https://github.com/tinyhumansai/medulla");
                        match client.connect_waitlist_github().await {
                            Ok(link) => { open_browser(&link.oauth_url); message = Some("star the repo, finish GitHub linking, then press v".into()); }
                            Err(error) => message = Some(error.to_string()),
                        }
                    }
                    KeyCode::Char('v') => match client.verify_waitlist_github_star().await {
                        Ok(next) => { status = next; message = Some("GitHub star verified".into()); }
                        Err(error) => message = Some(error.to_string()),
                    },
                    _ => {}
                }
            }
            _ = poll.tick() => {
                match client.waitlist_status().await {
                    Ok(next) => status = next,
                    Err(error) => message = Some(format!("refresh failed: {error}")),
                }
            }
        }
    }
}

fn draw(
    frame: &mut ratatui::Frame,
    status: &WaitlistStatus,
    invite: &str,
    entering_invite: bool,
    message: Option<&str>,
) {
    let area = crate::ui::layout::centered_fixed(70, 20, frame.area());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" medulla waitlist ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mark = |done: bool| if done { "✓" } else { "○" };
    let mut lines = vec![
        Line::from(Span::styled(
            "EARLY ACCESS",
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "Position  #{}    Priority score  {}",
            status.position.unwrap_or(0),
            status.priority_score
        )),
        Line::from(""),
        Line::from(format!(
            "{} Confirmed payment or subscription  +100",
            mark(status.boosts.paid.applied)
        )),
        Line::from(format!(
            "{} Claude/Codex power-user history    +50",
            mark(status.boosts.power_user.applied)
        )),
        Line::from(format!(
            "{} Star tinyhumansai/medulla           +25",
            mark(status.boosts.github_star.applied)
        )),
        Line::from(""),
        Line::from("i invite code · p upload history · g link GitHub · v verify star"),
        Line::from("r refresh · q quit · status refreshes every 30 seconds"),
    ];
    if entering_invite {
        lines.push(Line::from(""));
        lines.push(Line::from(format!("invite > {invite}")));
    }
    if let Some(message) = message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), inner);
}
