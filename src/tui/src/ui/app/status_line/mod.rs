//! The Status line settings page's model: its rows, what changing one does, and
//! how each change is written back.
//!
//! Every row is one field of [`StatusLineConfig`], and changing any of them is
//! the same operation — cycle the value, apply it live, persist the single key
//! that moved. Keeping that uniform is what lets the page grow a row without
//! growing a code path.
//!
//! Persistence writes one key at a time rather than the whole `[statusLine]`
//! table, because the config path is usually `medulla.tui.json` and only
//! [`persist_setting`](medulla::config::persist_setting) understands both the
//! JSON and TOML forms.

use medulla::config::{
    wire_value, ControlStyle, FieldPlacement, FieldVisibility, HarnessNameStyle, PathStyle,
    StatusLineConfig,
};

use super::types::App;

mod types;

#[cfg(test)]
mod tests;

pub(super) use types::*;

/// The page's rows, in display order, grouped by the field they describe.
///
/// Every group asks the same questions in the same order — where it sits, when
/// it is drawn, and (where there is more than one sensible spelling) how it is
/// written — so moving down the page is moving through one repeated form rather
/// than through fifteen unrelated switches.
pub(super) const STATUS_LINE_ROWS: [StatusLineRow; 15] = [
    row(
        "position",
        StatusLineField::State,
        head("State glyph", "the ●/✓/✕ dot: running, finished, or failed"),
        "Which line the state glyph sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::StateWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "position",
        StatusLineField::Harness,
        head("Harness name", "which CLI is driving the session"),
        "Which line the harness name sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::HarnessWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "spelled",
        StatusLineField::HarnessStyle,
        None,
        "Claude Code, claude, or just the provider icon.",
    ),
    row(
        "position",
        StatusLineField::Control,
        head(
            "Managed / unmanaged",
            "whether Medulla or you are driving the session",
        ),
        "Which line the control state sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::ControlWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "spelled",
        StatusLineField::ControlStyle,
        None,
        "Words (unmanaged), or the ⊘ / ⊙ symbols.",
    ),
    row(
        "position",
        StatusLineField::Thread,
        head("Thread name", "the conversation this session is working on"),
        "Which line the thread name sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::ThreadWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "position",
        StatusLineField::Branch,
        head("Git branch", "the branch the checkout is on"),
        "Which line the branch sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::BranchWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "position",
        StatusLineField::Path,
        head("Working path", "the checkout the harness is running in"),
        "Which line the working path sits on, or hide it.",
    ),
    row(
        "shown",
        StatusLineField::PathWhen,
        None,
        "On every row, only the selected one, or only alerts.",
    ),
    row(
        "spelled",
        StatusLineField::PathStyle,
        None,
        "The whole path, ~/…/tail, or the last segment alone.",
    ),
];

/// Number of selectable rows on the Status line page.
pub(super) const STATUS_LINE_ROW_COUNT: usize = STATUS_LINE_ROWS.len();

/// `const fn` shorthand so the table above reads as a table.
const fn row(
    label: &'static str,
    field: StatusLineField,
    group: Option<StatusLineGroup>,
    help: &'static str,
) -> StatusLineRow {
    StatusLineRow {
        label,
        field,
        group,
        help,
    }
}

/// `const fn` shorthand for the heading a group's first row opens.
const fn head(title: &'static str, description: &'static str) -> Option<StatusLineGroup> {
    Some(StatusLineGroup { title, description })
}

impl StatusLineField {
    /// Every value this field can take, in the order `←/→` walks them.
    ///
    /// The page shows the whole set for the selected row, because a row that
    /// displays only its current value gives an operator no way to learn what
    /// else is on offer short of pressing a key and watching the preview. Built
    /// from each enum's `ALL`, so the advertised list cannot drift from the
    /// sequence [`cycle`](Self::cycle) actually produces.
    pub(super) fn choices(self) -> Vec<&'static str> {
        match self {
            StatusLineField::State
            | StatusLineField::Harness
            | StatusLineField::Control
            | StatusLineField::Thread
            | StatusLineField::Branch
            | StatusLineField::Path => FieldPlacement::ALL.iter().map(|v| v.label()).collect(),
            StatusLineField::StateWhen
            | StatusLineField::HarnessWhen
            | StatusLineField::ControlWhen
            | StatusLineField::ThreadWhen
            | StatusLineField::BranchWhen
            | StatusLineField::PathWhen => FieldVisibility::ALL.iter().map(|v| v.label()).collect(),
            StatusLineField::HarnessStyle => {
                HarnessNameStyle::ALL.iter().map(|v| v.label()).collect()
            }
            StatusLineField::ControlStyle => ControlStyle::ALL.iter().map(|v| v.label()).collect(),
            StatusLineField::PathStyle => PathStyle::ALL.iter().map(|v| v.label()).collect(),
        }
    }

    /// The config key this field persists under, matching serde's camelCase
    /// spelling of [`StatusLineConfig`].
    pub(super) fn key(self) -> &'static str {
        match self {
            StatusLineField::State => "state",
            StatusLineField::StateWhen => "stateWhen",
            StatusLineField::Harness => "harness",
            StatusLineField::HarnessWhen => "harnessWhen",
            StatusLineField::HarnessStyle => "harnessStyle",
            StatusLineField::Control => "control",
            StatusLineField::ControlWhen => "controlWhen",
            StatusLineField::ControlStyle => "controlStyle",
            StatusLineField::Thread => "thread",
            StatusLineField::ThreadWhen => "threadWhen",
            StatusLineField::Branch => "branch",
            StatusLineField::BranchWhen => "branchWhen",
            StatusLineField::Path => "path",
            StatusLineField::PathWhen => "pathWhen",
            StatusLineField::PathStyle => "pathStyle",
        }
    }

    /// This field's full name, for the status bar.
    ///
    /// Distinct from the row label, which is abbreviated to "shown" / "spelled"
    /// because the page indents it under the field it belongs to. A status
    /// message has no such context — "Status line · shown → always" names
    /// nothing.
    fn name(self) -> &'static str {
        match self {
            StatusLineField::State => "state glyph",
            StatusLineField::StateWhen => "state glyph shown",
            StatusLineField::Harness => "harness name",
            StatusLineField::HarnessWhen => "harness name shown",
            StatusLineField::HarnessStyle => "harness name spelled",
            StatusLineField::Control => "managed / unmanaged",
            StatusLineField::ControlWhen => "managed / unmanaged shown",
            StatusLineField::ControlStyle => "managed / unmanaged spelled",
            StatusLineField::Thread => "thread name",
            StatusLineField::ThreadWhen => "thread name shown",
            StatusLineField::Branch => "git branch",
            StatusLineField::BranchWhen => "git branch shown",
            StatusLineField::Path => "working path",
            StatusLineField::PathWhen => "working path shown",
            StatusLineField::PathStyle => "working path spelled",
        }
    }

    /// This field's current value on `cfg`, as the label the page shows and the
    /// string that is persisted.
    pub(super) fn value(self, cfg: &StatusLineConfig) -> (&'static str, String) {
        match self {
            StatusLineField::State => (cfg.state.label(), wire_value(&cfg.state)),
            StatusLineField::StateWhen => (cfg.state_when.label(), wire_value(&cfg.state_when)),
            StatusLineField::Harness => (cfg.harness.label(), wire_value(&cfg.harness)),
            StatusLineField::HarnessWhen => {
                (cfg.harness_when.label(), wire_value(&cfg.harness_when))
            }
            StatusLineField::HarnessStyle => {
                (cfg.harness_style.label(), wire_value(&cfg.harness_style))
            }
            StatusLineField::Control => (cfg.control.label(), wire_value(&cfg.control)),
            StatusLineField::ControlWhen => {
                (cfg.control_when.label(), wire_value(&cfg.control_when))
            }
            StatusLineField::ControlStyle => {
                (cfg.control_style.label(), wire_value(&cfg.control_style))
            }
            StatusLineField::Thread => (cfg.thread.label(), wire_value(&cfg.thread)),
            StatusLineField::ThreadWhen => (cfg.thread_when.label(), wire_value(&cfg.thread_when)),
            StatusLineField::Branch => (cfg.branch.label(), wire_value(&cfg.branch)),
            StatusLineField::BranchWhen => (cfg.branch_when.label(), wire_value(&cfg.branch_when)),
            StatusLineField::Path => (cfg.path.label(), wire_value(&cfg.path)),
            StatusLineField::PathWhen => (cfg.path_when.label(), wire_value(&cfg.path_when)),
            StatusLineField::PathStyle => (cfg.path_style.label(), wire_value(&cfg.path_style)),
        }
    }

    /// Step this field's value on `cfg` one place, forwards or backwards.
    fn cycle(self, cfg: &mut StatusLineConfig, forward: bool) {
        match self {
            StatusLineField::State => cfg.state = cfg.state.cycled(forward),
            StatusLineField::StateWhen => cfg.state_when = cfg.state_when.cycled(forward),
            StatusLineField::Harness => cfg.harness = cfg.harness.cycled(forward),
            StatusLineField::HarnessWhen => cfg.harness_when = cfg.harness_when.cycled(forward),
            StatusLineField::HarnessStyle => {
                cfg.harness_style = cfg.harness_style.cycled(forward);
            }
            StatusLineField::Control => cfg.control = cfg.control.cycled(forward),
            StatusLineField::ControlWhen => cfg.control_when = cfg.control_when.cycled(forward),
            StatusLineField::ControlStyle => {
                cfg.control_style = cfg.control_style.cycled(forward);
            }
            StatusLineField::Thread => cfg.thread = cfg.thread.cycled(forward),
            StatusLineField::ThreadWhen => cfg.thread_when = cfg.thread_when.cycled(forward),
            StatusLineField::Branch => cfg.branch = cfg.branch.cycled(forward),
            StatusLineField::BranchWhen => cfg.branch_when = cfg.branch_when.cycled(forward),
            StatusLineField::Path => cfg.path = cfg.path.cycled(forward),
            StatusLineField::PathWhen => cfg.path_when = cfg.path_when.cycled(forward),
            StatusLineField::PathStyle => cfg.path_style = cfg.path_style.cycled(forward),
        }
    }
}

impl App {
    /// The status-line layout the settings page is editing.
    ///
    /// Materializes the derived-from-`[appearance]` layout on first read so the
    /// page always has a concrete value to show, without writing anything: an
    /// operator who only *looks* at the page should not have their config
    /// rewritten.
    pub(super) fn status_line_config(&self) -> StatusLineConfig {
        self.loaded.config.status_line()
    }

    /// Move the Status line page's selection.
    pub(super) fn move_status_line_index(&mut self, up: bool) {
        self.status_line_index = if up {
            self.status_line_index.saturating_sub(1)
        } else {
            (self.status_line_index + 1).min(STATUS_LINE_ROW_COUNT - 1)
        };
    }

    /// Cycle the selected row's value, apply it live, and persist it.
    pub(super) fn cycle_status_line_row(&mut self, forward: bool) {
        let row = STATUS_LINE_ROWS[self.status_line_index.min(STATUS_LINE_ROW_COUNT - 1)];
        let mut cfg = self.status_line_config();
        row.field.cycle(&mut cfg, forward);
        // Write the whole struct back: the first edit is also what promotes a
        // config that only had the legacy `[appearance]` booleans into an
        // explicit `[statusLine]` section.
        self.loaded.config.status_line = Some(cfg);
        let (label, wire) = row.field.value(&cfg);
        self.persist_status_line_now(&cfg, row.field.key(), row.field.name(), label, wire);
    }

    /// Write one status-line key to the injected config path.
    ///
    /// Reports the target in the status bar either way: config is layered, so a
    /// higher-precedence file can still override what was just written, and an
    /// operator whose change did not stick needs to know which file was tried.
    fn persist_status_line_now(
        &mut self,
        cfg: &StatusLineConfig,
        key: &str,
        row: &str,
        label: &str,
        wire: String,
    ) {
        match &self.config_path {
            Some(path) => {
                let result = if self.status_line_promotion_pending {
                    toml::Value::try_from(cfg)
                        .map_err(anyhow::Error::from)
                        .and_then(|value| match value {
                            toml::Value::Table(table) => {
                                medulla::config::persist_section(path, "statusLine", table)
                            }
                            _ => Err(anyhow::anyhow!(
                                "status-line config must serialize as a table"
                            )),
                        })
                } else {
                    medulla::config::persist_setting(
                        path,
                        "statusLine",
                        key,
                        toml::Value::String(wire),
                    )
                };
                match result {
                    Ok(()) => {
                        self.status_line_promotion_pending = false;
                        self.set_status(format!("Status line · {row} → {label} (saved)"));
                    }
                    Err(error) => self.set_status(format!("Status line · save failed: {error}")),
                }
            }
            None => self.set_status(format!("Status line · {row} → {label} (not persisted)")),
        }
    }
}
