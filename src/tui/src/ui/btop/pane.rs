//! Starting, showing, hiding and draining btop.

use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyEvent, MouseEvent};
use medulla_btop::{Btop, StartError};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::feed::AgentFeed;
use super::input::{key_bytes, mouse_bytes};
use super::render::blit;
use super::types::{BtopPane, Lifecycle, Waker};

/// How long [`BtopPane::shutdown`] waits for btop to save its config.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

impl BtopPane {
    /// Wake `waker` whenever btop draws. Takes effect at start, so set it
    /// before the tab is first shown.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Show btop in `area`, starting it on first use with its config under
    /// `config_dir`. Resizes btop and the emulator when `area` changed.
    pub fn show(&mut self, config_dir: &Path, area: Rect) {
        let size = (area.width.max(1), area.height.max(1));
        if matches!(self.lifecycle, Lifecycle::Idle) {
            self.size = size;
            self.screen.set_size(size.1, size.0);
            self.start(config_dir);
        }
        if size != self.size {
            self.size = size;
            self.screen.set_size(size.1, size.0);
            if let Lifecycle::Running { btop, .. } = &self.lifecycle {
                btop.resize(size.0, size.1);
            }
        }
        self.set_visible(true);
    }

    /// Pause btop while its tab is off screen.
    pub fn hide(&mut self) {
        self.set_visible(false);
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if let Lifecycle::Running { btop, feed } = &self.lifecycle {
            btop.set_visible(visible);
            feed.set_visible(visible);
        }
    }

    fn start(&mut self, config_dir: &Path) {
        let waker = self.waker.clone();
        let started = Btop::start(config_dir, self.size.0, self.size.1, move || {
            if let Some(waker) = &waker {
                waker();
            }
        });
        self.lifecycle = match started {
            Ok(btop) => {
                let feed = AgentFeed::spawn(|agents| super::pane().publish_agents(agents));
                Lifecycle::Running { btop, feed }
            }
            Err(error) => Lifecycle::Failed(start_failure(&error)),
        };
        // A fresh btop is visible from its first frame.
        self.visible = true;
    }

    /// Forward the feed's agent table to btop.
    fn publish_agents(&self, agents: &[medulla_btop::AgentTag]) {
        if let Lifecycle::Running { btop, .. } = &self.lifecycle {
            btop.set_agents(agents);
        }
    }

    /// Move btop's pending output into the emulator. Returns true when the
    /// operator asked btop to quit, which the host treats as leaving the tab.
    pub fn pump(&mut self) -> bool {
        let Lifecycle::Running { btop, .. } = &self.lifecycle else {
            return false;
        };
        let screen = &mut self.screen;
        btop.drain(|bytes| screen.process(bytes));
        let leave = btop.take_leave();
        if let Some(error) = btop.error() {
            self.lifecycle = Lifecycle::Failed(format!("btop stopped: {error}"));
        }
        leave
    }

    /// Draw the current picture, or why there is none, into `area`.
    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        match &self.lifecycle {
            Lifecycle::Running { .. } => blit(self.screen.screen(), buf, area),
            Lifecycle::Failed(message) => message_lines(buf, area, message),
            Lifecycle::Idle => {}
        }
    }

    /// Send `key` to btop. False when btop is not running or has no use for it.
    pub fn key(&self, key: KeyEvent) -> bool {
        match (&self.lifecycle, key_bytes(key)) {
            (Lifecycle::Running { btop, .. }, Some(bytes)) => {
                btop.send(&bytes);
                true
            }
            _ => false,
        }
    }

    /// Send a mouse event inside `area` (btop's screen) to btop.
    pub fn mouse(&self, event: MouseEvent, area: Rect) -> bool {
        match (&self.lifecycle, mouse_bytes(event, area)) {
            (Lifecycle::Running { btop, .. }, Some(bytes)) => {
                btop.send(&bytes);
                true
            }
            _ => false,
        }
    }

    /// Whether btop is running (as opposed to idle or failed).
    pub fn running(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Running { .. })
    }

    /// The text currently on btop's screen, row by row; for tests and logs.
    pub fn contents(&self) -> String {
        self.screen.screen().contents()
    }

    /// Stop btop so it saves its config. Called once as the app exits.
    pub fn shutdown(&mut self) {
        if let Lifecycle::Running { btop, .. } = &self.lifecycle {
            btop.stop(STOP_TIMEOUT);
        }
        self.lifecycle = Lifecycle::Failed("btop has stopped".into());
    }
}

/// What the tab says when btop cannot run.
fn start_failure(error: &StartError) -> String {
    match error {
        StartError::Unsupported => {
            "The Resources tab runs btop, which is not available on this platform.".into()
        }
        other => format!("btop could not start: {other}"),
    }
}

/// Centre `message` in `area`.
fn message_lines(buf: &mut Buffer, area: Rect, message: &str) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = message.chars().count().min(usize::from(area.width)) as u16;
    let x = area.x + (area.width - width) / 2;
    let y = area.y + area.height / 2;
    buf.set_stringn(
        x,
        y,
        message,
        usize::from(area.width),
        ratatui::style::Style::default().fg(ratatui::style::Color::Yellow),
    );
}
