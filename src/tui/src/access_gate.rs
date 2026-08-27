//! The launch-time access check: may this account use Medulla?
//!
//! Split out of [`crate::app_loop`] for the same reason [`crate::sign_in`] was:
//! it is a separate question from wiring up the app. Signing in establishes
//! *who* the operator is; this establishes whether that account is entitled to
//! what it just signed in to.
//!
//! # Why it runs on every launch, not just after a login
//!
//! Entitlement changes without the operator touching the terminal — a trial runs
//! out overnight, a card is declined, a subscription is cancelled from the web.
//! A check that only ran at sign-in would admit an account that stopped being
//! entitled weeks ago, because the session it signed in with is still perfectly
//! valid. So it hangs off the boot path, after a session is resolved, and asks
//! the backend fresh each time.
//!
//! # A backend it cannot reach is a refusal
//!
//! Entitlement is not something this binary can decide alone, so a launch that
//! cannot ask does not get to assume. The operator is shown the same refusal as
//! an unresolvable plan, phrased as what it is — we could not confirm — rather
//! than as an expired trial.
//!
//! # What this is not
//!
//! It is not the enforcement point — see [`medulla::access`]. What an account
//! may actually spend is metered by the backend on the routes that cost money,
//! where a patched client cannot argue. This decides what the terminal shows,
//! and its job is to explain the situation rather than to hold a line.

use std::io;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use medulla::access::{Access, Denial, Grant};
use medulla::auth::{describe_me, open_browser};
use medulla::client::MedullaClient;
use medulla_tui::ui::upgrade::{UpgradeCmd, UpgradeOutcome, UpgradeScreen};

/// What the gate decided about this launch.
pub(crate) enum Gate {
    /// Carry on into the app. Carries the startup note to show, when the grant
    /// is one worth mentioning — a trial counting down.
    Allowed(Option<String>),
    /// Stop. The operator saw the paywall screen and chose to leave.
    Refused,
}

/// Check `session`'s entitlement and, when it is refused, show the paywall.
///
/// Runs inside the caller's alt-screen session, borrowing the terminal the same
/// way the login screen does, so there is no flicker between them.
///
/// # Errors
///
/// Propagates terminal draw failures only. A backend that cannot be reached is
/// not an error either — it is a refusal, shown as [`Denial::Undetermined`]:
/// a launch that could not ask whether this account is entitled does not get to
/// assume that it is.
pub(crate) async fn check(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &medulla::auth::Credentials,
) -> anyhow::Result<Gate> {
    let client = MedullaClient::new(session.base_url.clone(), session.jwt.clone());
    let me = match client.me().await {
        Ok(me) => me,
        // Unreachable, rate-limited, 500. None of them say this account pays,
        // and none of them say its free week is up either — so it is refused as
        // the unconfirmed case, under the account's own name where we have one.
        Err(_) => {
            return run_upgrade_screen(terminal, who(session), Denial::Undetermined)
                .await
                .map(|UpgradeOutcome::Quit| Gate::Refused)
        }
    };

    match medulla::access::access_from_me(&me) {
        Access::Granted(grant) => Ok(Gate::Allowed(startup_note(grant))),
        Access::Denied(denial) => run_upgrade_screen(terminal, describe_me(&me), denial)
            .await
            .map(|UpgradeOutcome::Quit| Gate::Refused),
    }
}

/// The best "who am I" line available without a backend to ask.
///
/// `/auth/me` is what normally phrases this, so when that is the call that
/// failed there is nothing to name the operator with — the deployment they were
/// talking to is the one fact still in hand, and it is the one that matters for
/// telling a staging mix-up from a real outage.
fn who(session: &medulla::auth::Credentials) -> String {
    format!("backend {}", session.base_url)
}

/// The one-line startup note a grant is worth announcing with, if any.
///
/// Only the trial gets one, and only because it is the grant with a deadline:
/// an operator whose free week ends on Thursday should not discover that on
/// Thursday. A paid or allowlisted account needs no notice that it is fine.
fn startup_note(grant: Grant) -> Option<String> {
    let Grant::Trial { remaining_ms } = grant else {
        return None;
    };
    // Round up: with eleven hours left it is more useful — and more honest — to
    // say "1 day left" than "0 days left".
    let day_ms = 24 * 60 * 60 * 1_000_i64;
    let days = remaining_ms.div_euclid(day_ms) + i64::from(remaining_ms.rem_euclid(day_ms) > 0);
    Some(match days {
        1 => "Free trial: 1 day left. Subscribe at https://tinyhumans.ai/pricing".to_string(),
        n => format!("Free trial: {n} days left. Subscribe at https://tinyhumans.ai/pricing"),
    })
}

/// Draw the paywall until the operator leaves it.
async fn run_upgrade_screen(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    who: String,
    denial: Denial,
) -> anyhow::Result<UpgradeOutcome> {
    use crossterm::event::{Event, EventStream, KeyEventKind};
    use futures::StreamExt;

    let mut screen = UpgradeScreen::new(who, denial);
    let mut reader = EventStream::new();

    loop {
        terminal.draw(|f| screen.draw(f))?;
        if let Some(outcome) = screen.outcome() {
            return Ok(outcome);
        }
        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            // Best-effort, like the login screen's link rows: a browser that
            // will not launch must not trap the operator on this screen.
            if let Some(UpgradeCmd::OpenUrl(url)) = screen.handle_key(key) {
                open_browser(&url);
            }
        }
    }
}

#[cfg(test)]
#[path = "access_gate_tests.rs"]
mod tests;
