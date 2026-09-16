//! Drawing the Hosts tab: a list on the left, the selected host on the right,
//! and the pairing command boxed where it cannot be missed.
//!
//! - [`list`] — the rows.
//! - [`detail`] — one host's declaration, node ids, and its key.
//! - [`switcher`] — the `H` overlay, drawn over any tab.

mod detail;
mod list;
mod switcher;

#[cfg(test)]
mod tests;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;

use super::super::types::App;

impl App {
    /// Draw the Hosts tab.
    ///
    /// The rows are rebuilt on every frame: they are config times a map
    /// lookup, and rebuilding is what keeps a liveness change on the rail's
    /// side visible here without another message to carry it.
    pub(super) fn draw_hosts_tab(&mut self, f: &mut Frame, area: Rect) {
        self.rebuild_host_entries();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
            .split(area);
        self.draw_host_list(f, columns[0]);
        self.draw_host_detail(f, columns[1]);
    }
}
