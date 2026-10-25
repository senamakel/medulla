use super::agents::AgentRow;
use crate::ui::app::resources::chrome::{self, Bar};
use crate::ui::theme::ResourcesTheme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    Frame,
};

/// Draw the selected agent's details inside the Agents box.
pub(super) fn draw(frame: &mut Frame, area: Rect, row: Option<&AgentRow>, theme: &ResourcesTheme) {
    let buffer = frame.buffer_mut();
    let inner = chrome::frame(buffer, area, theme.proc_box);
    let mut top = Bar::top(area, theme.proc_box);
    top.left(buffer, &chrome::plain(theme, "Agent detail"));
    top.right(buffer, &chrome::keyed(theme, "Esc back", 'E', true));
    Bar::bottom(area, theme.proc_box).left(buffer, &chrome::plain(theme, "t/k/s signal"));
    if inner.is_empty() {
        return;
    }
    let label = Style::default().fg(theme.main_fg);
    let value = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD);
    let Some(row) = row else {
        chrome::text(
            buffer,
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            "No agent selected",
            Style::default().fg(theme.inactive_fg),
        );
        return;
    };
    let fields = [
        ("PID", row.pid.to_string()),
        ("Agent", row.kind.clone()),
        ("Origin", row.origin.clone()),
        ("Workspace", row.workspace.clone()),
        ("Threads", row.threads.to_string()),
        ("Memory", format!("{} bytes", row.memory)),
        ("CPU", format!("{:.1}%", row.cpu)),
        ("Runtime", format!("{} seconds", row.runtime)),
        ("Parent", row.ppid.map_or("—".into(), |pid| pid.to_string())),
    ];
    for (index, (name, text)) in fields.iter().enumerate() {
        let y = inner.y + index as u16;
        if y >= inner.bottom() {
            break;
        }
        let x = chrome::text(buffer, inner.x + 1, y, 11, &format!("{name:<10}"), label);
        let style = if *name == "CPU" {
            Style::default().fg(chrome::shade(&theme.process, row.cpu))
        } else {
            value
        };
        chrome::text(buffer, x, y, inner.right().saturating_sub(x), text, style);
    }
}
