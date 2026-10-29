//! The btop pane's state.

use std::sync::Arc;

use medulla_btop::Btop;

use super::feed::AgentFeed;

/// Wakes the UI loop when btop has drawn something.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Where btop is in its once-per-process life.
#[derive(Default)]
pub enum Lifecycle {
    /// Not started yet; the first time the tab is shown starts it.
    #[default]
    Idle,
    /// Running, with the agent feed that keeps its agents box current.
    Running {
        /// The handle over btop's C ABI.
        btop: Btop,
        /// The background agent classifier.
        feed: AgentFeed,
    },
    /// Could not start, or btop's thread died; the message says why.
    Failed(String),
}

/// The host side of btop's virtual terminal.
pub struct BtopPane {
    /// Where btop is in its life.
    pub lifecycle: Lifecycle,
    /// The emulator btop's output is drained into.
    pub screen: vt100::Parser,
    /// The size last reported to btop and the emulator, as (cols, rows).
    pub size: (u16, u16),
    /// Whether the tab is on screen; btop pauses while it is not.
    pub visible: bool,
    /// Called from btop's threads when output is ready.
    pub waker: Option<Waker>,
}

impl Default for BtopPane {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::Idle,
            screen: vt100::Parser::new(24, 80, 0),
            size: (80, 24),
            visible: false,
            waker: None,
        }
    }
}
