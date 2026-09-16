//! Slash-command parsing and the command catalog.
//!
//! [`parse`] classifies a raw composer line into a [`SlashCommand`] without
//! touching any UI state; the front end matches on the result to run the side
//! effect. Keeping the parse pure makes the command surface testable and
//! reusable across front ends.

mod catalog;
mod types;

#[cfg(test)]
mod tests;

pub use catalog::{lookup, suggestions, CommandSpec, COMMANDS};
pub use types::{CopyScope, SlashCommand};

use crate::protocol::HarnessProvider;

impl SlashCommand {
    /// Parse a composer line into a command; see [`parse`] (module function) for
    /// the full contract. `None` means "not a slash command".
    pub fn parse(input: &str) -> Option<SlashCommand> {
        parse(input)
    }
}

/// Parse a composer line into a [`SlashCommand`].
///
/// Returns `None` when `input` is not a slash command (no leading `/` after
/// trimming) so the caller can treat it as a normal prompt. The command token is
/// matched case-insensitively; free-text arguments (`/session <path>`) preserve
/// their original case. Unrecognized commands map to [`SlashCommand::Unknown`]
/// and invalid arguments to [`SlashCommand::BadUsage`], so no input is silently
/// dropped.
pub fn parse(input: &str) -> Option<SlashCommand> {
    let rest = input.trim().strip_prefix('/')?.trim();
    let (cmd_raw, arg) = match rest.split_once(char::is_whitespace) {
        Some((c, a)) => (c, a.trim()),
        None => (rest, ""),
    };
    let cmd = cmd_raw.to_lowercase();
    Some(match cmd.as_str() {
        "quit" | "q" | "exit" => SlashCommand::Quit,
        "session" | "harness" => parse_session(arg),
        "help" => SlashCommand::Help,
        "config" => SlashCommand::Config,
        "settings" | "theme" => SlashCommand::Settings,
        "usage" => SlashCommand::Usage,
        "feedback" | "fb" => SlashCommand::Feedback,
        "mouse" => SlashCommand::ToggleMouse,
        _ => SlashCommand::Unknown(input.trim().to_string()),
    })
}

/// Parse the argument tail of `/session` into its harness type and path.
///
/// The shapes are `/session`, `/session <harness>`, and
/// `/session <harness> <path>`. A named harness type is validated here, against
/// the same [`HarnessProvider::from_wire`] the wire uses, because "claud" should
/// say so rather than silently starting the default CLI — that failure is
/// invisible until the wrong one is already running in the operator's workspace.
///
/// The path is not validated: only the front end knows the active workspace, and
/// a bad path produces a far better message at spawn time than at parse time.
fn parse_session(arg: &str) -> SlashCommand {
    const USAGE: &str = "Usage: /session [claude|codex|opencode] [path]";
    let arg = arg.trim();
    if arg.is_empty() {
        return SlashCommand::StartSession {
            provider: None,
            path: None,
        };
    }
    let (provider_raw, path) = match arg.split_once(char::is_whitespace) {
        Some((p, rest)) => (p, non_empty(rest)),
        None => (arg, None),
    };
    let provider = provider_raw.to_lowercase();
    if HarnessProvider::from_wire(&provider).is_none() {
        return SlashCommand::BadUsage(USAGE);
    }
    SlashCommand::StartSession {
        provider: Some(provider),
        path,
    }
}

/// `Some(trimmed)` when `s` has non-whitespace content, else `None`.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
