//! The parsed slash-command vocabulary.

/// A parsed slash command entered on the composer line.
///
/// [`super::parse`] turns raw input into one of these; the front end matches on
/// the result to perform the side effect. Parsing is pure and carries no UI
/// state, so the same vocabulary is reusable by any front end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    /// `/quit`, `/exit`, `/q` — exit the application.
    Quit,
    /// `/session [harness] [path]` — start a local session.
    ///
    /// Both arguments are optional: with neither, the front end opens its
    /// picker. Parsing does not validate the path — only the front end knows
    /// what the active workspace is, and a path that does not exist is a
    /// spawn-time error with a much better message than a parse-time one.
    StartSession {
        /// The harness type to run, lowercased, when one was named.
        provider: Option<String>,
        /// The working directory to start it in, when one was given.
        path: Option<String>,
    },
    /// `/help` — show the help subpage.
    Help,
    /// `/config` — show the config subpage.
    Config,
    /// `/settings`, `/theme` — show the appearance subpage.
    Settings,
    /// `/usage` — show the usage subpage (fetches account usage on entry).
    Usage,
    /// `/feedback`, `/fb` — open the feedback board.
    Feedback,
    /// `/mouse` — toggle mouse capture.
    ToggleMouse,
    /// A recognized command invoked with an invalid argument; carries the usage
    /// hint to surface.
    BadUsage(&'static str),
    /// An unrecognized command; carries the original trimmed input for the error
    /// message.
    Unknown(String),
}
