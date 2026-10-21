use serde::{Deserialize, Serialize};
/// How a provider's process is reached — the *flavor* of a harness, as distinct
/// from which harness it is.
///
/// A provider and a transport are independent choices. [`HarnessProvider`]
/// answers "which vendor's agent runs this", and stays the answer to every
/// question that follows from it: which credentials, which config overrides,
/// which inference endpoint, which seat the tokens are billed to. This answers
/// the separate question of *how the process is driven*, which changes only the
/// execution seam.
///
/// Keeping them apart is what lets `codex-server` exist without a second Codex
/// everywhere: an app-server run is still Codex for auth, hooks, attribution,
/// and budget, and differs only in that it shares one long-lived process instead
/// of forking a CLI per task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessTransport {
    /// Spawn the provider's CLI once per task and read its streaming JSONL.
    ///
    /// The default, and the only transport every provider supports.
    #[default]
    Cli,
    /// Open a thread on a shared, long-lived `codex app-server` process.
    ///
    /// One process serves many concurrent threads, so N lanes cost one model
    /// runtime rather than N. Codex only — see
    /// [`HarnessTransport::supported_by`].
    AppServer,
}

impl HarnessTransport {
    /// The wire string for this transport.
    pub fn as_str(self) -> &'static str {
        match self {
            HarnessTransport::Cli => "cli",
            HarnessTransport::AppServer => "app_server",
        }
    }

    /// Parse a transport name, returning `None` for anything unrecognized.
    ///
    /// Both `app_server` (the wire form) and `app-server` (what an operator
    /// types) are accepted, because the same word reaches this from a frame and
    /// from a command line.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "cli" => Some(HarnessTransport::Cli),
            "app_server" | "app-server" => Some(HarnessTransport::AppServer),
            _ => None,
        }
    }

    /// Whether `provider` can be driven over this transport.
    ///
    /// Every provider supports [`Cli`](Self::Cli). Only Codex ships an
    /// app-server, so pairing [`AppServer`](Self::AppServer) with anything else
    /// is a configuration error the caller should refuse rather than silently
    /// downgrade — a run that quietly forks a CLI when the operator asked for
    /// the shared process would look like the feature working.
    pub fn supported_by(self, provider: HarnessProvider) -> bool {
        match self {
            HarnessTransport::Cli => true,
            HarnessTransport::AppServer => matches!(provider, HarnessProvider::Codex),
        }
    }

    /// Whether this is the provider-default transport, and so need not be
    /// stated on the wire.
    pub fn is_default(self) -> bool {
        matches!(self, HarnessTransport::Cli)
    }
}

/// The coding-agent CLI that ran (or should run) a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessProvider {
    /// Anthropic's `claude` CLI harness.
    Claude,
    /// OpenAI's `codex` CLI harness.
    Codex,
    /// The `opencode` CLI harness.
    Opencode,
    /// The in-process OpenHuman provider.
    ///
    /// The one provider with no binary behind it: a task dispatched to it runs
    /// as an in-process agent turn on the openhuman-embed harness rather than a
    /// spawned CLI — see [`crate::daemon::providers::embed`].
    ///
    /// It is dispatchable, but never *auto-selected*: it is not
    /// interchangeable with a coding CLI, so a task reaches it only by naming
    /// it. See [`crate::daemon::providers::detect_providers`].
    Openhuman,
    /// A plain interactive shell — `bash`, `zsh`, whatever `$SHELL` names.
    ///
    /// Not a coding agent, and deliberately kept out of every path that treats
    /// one as interchangeable with another: it is never detected as an
    /// available provider ([`crate::daemon::providers::DAEMON_PROVIDERS`]) and
    /// never dispatchable ([`is_dispatchable`](Self::is_dispatchable)), so no
    /// peer frame can reach a shell however it spells the harness name. It
    /// exists so an operator can open a terminal beside their agents, in the
    /// same pane, on the same host, with the same working directory — which
    /// until now meant leaving the TUI.
    Shell,
}

impl HarnessProvider {
    /// The wire string for this provider.
    pub fn as_str(&self) -> &'static str {
        match self {
            HarnessProvider::Claude => "claude",
            HarnessProvider::Codex => "codex",
            HarnessProvider::Opencode => "opencode",
            HarnessProvider::Openhuman => "openhuman",
            HarnessProvider::Shell => "shell",
        }
    }

    /// The human-readable product name, for UI labels.
    ///
    /// Distinct from [`HarnessProvider::as_str`], which is the lowercase wire
    /// identifier and must not change.
    pub fn display_name(&self) -> &'static str {
        match self {
            HarnessProvider::Claude => "Claude Code",
            HarnessProvider::Codex => "Codex",
            HarnessProvider::Opencode => "OpenCode",
            HarnessProvider::Openhuman => "OpenHuman",
            HarnessProvider::Shell => "Shell",
        }
    }

    /// A single-column glyph standing in for the provider name.
    ///
    /// For the status line's `icon` harness style: an operator running one
    /// provider does not need its name spelled on every row, and the columns it
    /// costs are the ones the working directory wants. Deliberately geometric
    /// rather than emoji — an emoji is two columns wide on some terminals and
    /// one on others, which would break the rail's width arithmetic.
    pub fn icon(&self) -> &'static str {
        match self {
            HarnessProvider::Claude => "✳",
            HarnessProvider::Codex => "◆",
            HarnessProvider::Opencode => "◻",
            HarnessProvider::Openhuman => "○",
            HarnessProvider::Shell => "$",
        }
    }

    /// The provider's wire name — the exact inverse of [`from_wire`].
    ///
    /// Distinct from [`as_str`](Self::as_str), which is the *display* name
    /// (`OpenCode`, capitalised for a rail row). This is the lowercase token
    /// that crosses a wire, and the two must not be confused: a frame carrying
    /// `"OpenCode"` does not parse back.
    ///
    /// [`from_wire`]: Self::from_wire
    pub fn as_wire(self) -> &'static str {
        match self {
            HarnessProvider::Claude => "claude",
            HarnessProvider::Codex => "codex",
            HarnessProvider::Opencode => "opencode",
            HarnessProvider::Openhuman => "openhuman",
            HarnessProvider::Shell => "shell",
        }
    }

    /// Parse a provider name, returning `None` for anything unrecognized.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(HarnessProvider::Claude),
            "codex" => Some(HarnessProvider::Codex),
            "opencode" => Some(HarnessProvider::Opencode),
            "openhuman" => Some(HarnessProvider::Openhuman),
            "shell" => Some(HarnessProvider::Shell),
            _ => None,
        }
    }

    /// Parse a provider that may receive delegated coding tasks.
    pub fn dispatchable_from_wire(value: &str) -> Option<Self> {
        Self::from_wire(value).filter(|provider| provider.is_dispatchable())
    }

    /// Whether this provider accepts delegated coding-task frames.
    ///
    /// Every coding agent does. The distinction this used to draw — OpenHuman
    /// excluded because it "owns its own agent loop" — did not survive
    /// contact with the fact that every harness here owns one; Claude Code's
    /// simply sits behind a process boundary instead of a function call. What
    /// is genuinely different about OpenHuman is that it is not
    /// *interchangeable* with a coding CLI, and that is expressed where it
    /// belongs: it is never auto-detected as an available provider, so a task
    /// reaches it only by naming it.
    ///
    /// [`Shell`](Self::Shell) is the one that answers `false`, and it is the
    /// reason this stayed a method. A shell reads no prompt and reports no
    /// completion, so a task frame naming one would be a turn that never
    /// answers — and a peer that could name one would have a way to run
    /// arbitrary commands on this host without a harness in between. Both are
    /// refused at the parse: [`dispatchable_from_wire`](Self::dispatchable_from_wire)
    /// returns `None` for `"shell"`.
    pub fn is_dispatchable(self) -> bool {
        !matches!(self, Self::Shell)
    }

    /// Whether running this provider means spawning an executable.
    ///
    /// True for every coding CLI, and the reason a host detects them by looking
    /// on `PATH`. False for OpenHuman, which is linked into this process — so
    /// asking `PATH` about it would answer `false` on a host that can run it
    /// perfectly well.
    pub fn needs_binary(self) -> bool {
        !matches!(self, Self::Openhuman)
    }

    /// Parse a harness *flavor* name into the provider it runs and the transport
    /// it runs over.
    ///
    /// A flavor is how an operator names a provider/transport pair in the one
    /// place they get to name anything: a workflow node's `harness:`, a config
    /// key, a fleet tool argument. Plain provider names keep their meaning and
    /// resolve to [`HarnessTransport::Cli`]; `codex-server` is Codex over the
    /// shared app-server.
    ///
    /// Returns `None` for anything unrecognized, exactly as
    /// [`from_wire`](Self::from_wire) does, so a caller can fall through to its
    /// custom-preset handling.
    pub fn flavor_from_wire(value: &str) -> Option<(Self, HarnessTransport)> {
        match value {
            // Both spellings, for the same reason `HarnessTransport::from_wire`
            // takes both: this name is typed by hand as often as it is decoded.
            CODEX_SERVER_FLAVOR | "codex_server" => {
                Some((HarnessProvider::Codex, HarnessTransport::AppServer))
            }
            // Dispatch-only, like every caller of this: a flavor is what a
            // workflow node, a fleet tool, or a task frame names as the thing
            // that should *run* something. `"shell"` parses nowhere there — see
            // [`is_dispatchable`](Self::is_dispatchable).
            _ => Self::dispatchable_from_wire(value)
                .map(|provider| (provider, HarnessTransport::Cli)),
        }
    }

    /// The flavor name for this provider/transport pair — the inverse of
    /// [`flavor_from_wire`](Self::flavor_from_wire).
    ///
    /// A default-transport pair is simply the provider name, so round-tripping
    /// an ordinary harness never invents a suffix nobody wrote.
    ///
    /// # Panics (debug builds only)
    ///
    /// `self` must be [`is_dispatchable`](Self::is_dispatchable). `flavor_from_wire`
    /// never *parses* a non-dispatchable provider's name back into a flavor —
    /// `"shell"` returns `None` — so a caller that fed one in here would get a
    /// string dispatch parsers reject, breaking the round trip this method
    /// promises. Every caller in this codebase already sources `self` from a
    /// dispatchable set (`dispatchable_flavors`, a parsed `TaskRequest`
    /// provider, a resolved `HarnessChoice`), so this should never fire
    /// outside a bug; the debug assertion catches that bug in tests rather
    /// than emitting a wire-invalid flavor in release.
    pub fn flavor_name(self, transport: HarnessTransport) -> &'static str {
        debug_assert!(
            self.is_dispatchable(),
            "flavor_name called on non-dispatchable provider {self:?}; \
             flavor_from_wire cannot parse its name back, breaking the round trip"
        );
        match (self, transport) {
            (HarnessProvider::Codex, HarnessTransport::AppServer) => CODEX_SERVER_FLAVOR,
            _ => self.as_str(),
        }
    }

    /// The human-readable product name for a flavor, for UI labels.
    pub fn flavor_display_name(self, transport: HarnessTransport) -> &'static str {
        match (self, transport) {
            (HarnessProvider::Codex, HarnessTransport::AppServer) => "Codex (shared server)",
            _ => self.display_name(),
        }
    }
}

/// The flavor name for Codex over the shared app-server.
///
/// Named once because it is simultaneously a wire value, a config value, an
/// error-message token, and a fleet-tool enum member — four places that must not
/// drift.
pub const CODEX_SERVER_FLAVOR: &str = "codex-server";

/// Every harness flavor a task may be dispatched to, in picker order.
///
/// Derived from [`HarnessProvider::is_dispatchable`] plus the non-default
/// transports each provider supports, so a new provider or transport shows up
/// here without a second list to remember.
pub fn dispatchable_flavors() -> Vec<(HarnessProvider, HarnessTransport)> {
    let mut flavors = Vec::new();
    for provider in [
        HarnessProvider::Claude,
        HarnessProvider::Codex,
        HarnessProvider::Opencode,
        HarnessProvider::Openhuman,
    ] {
        if !provider.is_dispatchable() {
            continue;
        }
        for transport in [HarnessTransport::Cli, HarnessTransport::AppServer] {
            if transport.supported_by(provider) {
                flavors.push((provider, transport));
            }
        }
    }
    flavors
}

