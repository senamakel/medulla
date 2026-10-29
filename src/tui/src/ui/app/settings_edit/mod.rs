//! The Config subpage's editing behaviour: which settings are editable, how to
//! read their current values out of the effective config, and how to apply and
//! persist a change.
//!
//! Editing is deliberately narrow. A config value is only exposed here when it
//! is a genuine user preference with a small, safe domain — a switch or a
//! bounded number. Paths, URLs, and credentials stay file-managed, since they
//! need validation this surface cannot offer and are set once rather than tuned.
//!
//! Every write goes through [`medulla::config::persist_setting`], which rewrites
//! only the one `[section]` key it owns. Serializing the whole [`TuiConfig`]
//! back would be wrong: the loader bakes env overrides and home-derived paths
//! into the in-memory struct, so a round-trip would pin those as if the user had
//! typed them.

use medulla::config::OpencodeConfig;

use super::types::App;

mod types;

#[cfg(test)]
mod tests;

pub(crate) use types::{SettingKind, SettingRow, SettingValue};

impl App {
    /// The editable rows for the Config subpage, in display order.
    pub(crate) fn config_rows(&self) -> Vec<SettingRow> {
        let mut rows = vec![SettingRow {
            label: "Update check",
            section: "update",
            key: "check",
            kind: SettingKind::Toggle,
            help: "Check for a newer Medulla release on startup.",
        }];
        rows.extend([SettingRow {
            label: "Worker concurrency",
            section: "opencode",
            key: "maxConcurrency",
            kind: SettingKind::Count {
                min: 1,
                max: 32,
                step: 1,
                fallback: 4,
                // The field is a plain u32 with a serde default, so it is
                // always set; there is no "auto" state to fall back to.
                optional: false,
            },
            help: "Agent sessions Medulla may run at once.",
        }]);
        // Only where there is a Workflows page for it to bound.
        #[cfg(feature = "workflows")]
        rows.push(SettingRow {
            label: "Runs listed",
            section: "workflows",
            key: "maxListedRuns",
            kind: SettingKind::Count {
                min: 1,
                max: 200,
                step: 5,
                fallback: medulla::config::WorkflowsConfig::DEFAULT_MAX_LISTED_RUNS as u32,
                // Always set, through the section's serde default, so there is
                // no "auto" state to step back into.
                optional: false,
            },
            help: "Recent runs shown under a workflow on the Workflows page.",
        });
        rows
    }

    /// The current value of `row`, read from the effective config.
    pub(crate) fn read_setting(&self, row: &SettingRow) -> SettingValue {
        let cfg = &self.loaded.config;
        match (row.section, row.key) {
            ("update", "check") => SettingValue::Flag(cfg.update.check),
            ("opencode", "maxConcurrency") => SettingValue::Number(
                cfg.opencode
                    .as_ref()
                    .map(|o| o.max_concurrency)
                    .unwrap_or_else(|| OpencodeConfig::default().max_concurrency),
            ),
            ("workflows", "maxListedRuns") => {
                SettingValue::Number(cfg.workflows.max_listed_runs as u32)
            }
            _ => SettingValue::Auto,
        }
    }

    /// Apply a change to the selected Config row and persist it.
    ///
    /// `delta` is `0` for a toggle/Enter, `-1`/`+1` for ←/→. Toggles flip on any
    /// input; counts step by their row's `step`, and an optional count stepped
    /// below its minimum returns to "auto" (clearing the key) rather than
    /// pinning the default.
    ///
    /// Returns the status message to surface, so the caller controls when it is
    /// shown.
    pub(crate) fn adjust_setting(&mut self, delta: i32) -> String {
        let rows = self.config_rows();
        let Some(row) = rows.get(self.config_index.min(rows.len().saturating_sub(1))) else {
            return "Config · nothing to edit".into();
        };
        let row = *row;

        let next = match row.kind {
            SettingKind::Toggle => {
                let SettingValue::Flag(current) = self.read_setting(&row) else {
                    return "Config · unexpected value".into();
                };
                SettingValue::Flag(!current)
            }
            SettingKind::Count {
                min,
                max,
                step,
                fallback,
                optional,
            } => {
                if delta == 0 {
                    return format!("Config · {} needs ← or → to change", row.label);
                }
                match self.read_setting(&row) {
                    // Stepping an unset field starts from its documented default
                    // rather than from zero.
                    SettingValue::Auto => SettingValue::Number(fallback.clamp(min, max)),
                    SettingValue::Number(current) => {
                        let raw = current as i64 + (delta as i64) * step as i64;
                        if optional && raw < min as i64 {
                            SettingValue::Auto
                        } else {
                            SettingValue::Number(raw.clamp(min as i64, max as i64) as u32)
                        }
                    }
                    SettingValue::Flag(_) => return "Config · unexpected value".into(),
                }
            }
        };

        self.write_setting(&row, next);
        self.persist_setting_now(&row, next)
    }

    /// Mirror `value` into the in-memory config so the change shows immediately,
    /// creating any optional section it belongs to.
    fn write_setting(&mut self, row: &SettingRow, value: SettingValue) {
        let cfg = &mut self.loaded.config;
        match (row.section, row.key, value) {
            ("update", "check", SettingValue::Flag(on)) => cfg.update.check = on,
            ("opencode", "maxConcurrency", SettingValue::Number(n)) => {
                cfg.opencode
                    .get_or_insert_with(OpencodeConfig::default)
                    .max_concurrency = n;
            }
            ("workflows", "maxListedRuns", SettingValue::Number(n)) => {
                cfg.workflows.max_listed_runs = n as usize;
            }
            _ => {}
        }
        // The Workflows page caches the runs it read, so a new cap only reaches
        // the rail on the next reload. Take it now: the operator is watching the
        // number they just changed.
        #[cfg(feature = "workflows")]
        if (row.section, row.key) == ("workflows", "maxListedRuns") {
            self.reload_workflow_runs();
        }
    }

    /// Write `value` to the injected config file, returning the status message.
    ///
    /// Reports the target path so it is clear which file in the layered load was
    /// written, and warns when a higher-precedence file may still override it.
    fn persist_setting_now(&self, row: &SettingRow, value: SettingValue) -> String {
        let Some(path) = &self.config_path else {
            return format!(
                "Config · {} → {} (applied live, no config path set)",
                row.label,
                value.display()
            );
        };
        let result = match value {
            SettingValue::Auto => medulla::config::clear_setting(path, row.section, row.key),
            SettingValue::Flag(on) => medulla::config::persist_setting(
                path,
                row.section,
                row.key,
                toml::Value::Boolean(on),
            ),
            SettingValue::Number(n) => medulla::config::persist_setting(
                path,
                row.section,
                row.key,
                toml::Value::Integer(n as i64),
            ),
        };
        match result {
            Err(e) => format!("Config · save failed: {e}"),
            Ok(()) => {
                let overridden = self.overriding_source(path);
                match overridden {
                    Some(other) => format!(
                        "Config · {} → {} (saved to {}, but {other} still overrides it)",
                        row.label,
                        value.display(),
                        path.display()
                    ),
                    None => format!(
                        "Config · {} → {} (saved to {})",
                        row.label,
                        value.display(),
                        path.display()
                    ),
                }
            }
        }
    }

    /// The highest-precedence config source that outranks `path`, if any.
    ///
    /// Settings are written to the user-global file, but a project-local config
    /// layered on top would win. Naming it beats silently saving a value the
    /// user will not see take effect.
    fn overriding_source(&self, path: &std::path::Path) -> Option<String> {
        let target = path.to_string_lossy();
        let position = self.loaded.sources.iter().position(|s| *s == target)?;
        self.loaded.sources.get(position + 1..)?.last().cloned()
    }

    /// The Config subpage's selected row index, clamped to the current rows.
    pub(crate) fn config_row_index(&self) -> usize {
        let len = self.config_rows().len();
        self.config_index.min(len.saturating_sub(1))
    }

    /// Move the Config subpage cursor by one row.
    pub(crate) fn move_config_index(&mut self, up: bool) {
        let max = self.config_rows().len().saturating_sub(1);
        self.config_index = if up {
            self.config_row_index().saturating_sub(1)
        } else {
            (self.config_row_index() + 1).min(max)
        };
    }
}
