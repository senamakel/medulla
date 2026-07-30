//! Workspace discovery, completion, and recent-history persistence for the
//! manual harness launcher.

use std::collections::HashSet;
use std::path::Path;

use super::types::{App, HarnessPickerStep, WorkspaceChoice};

const MAX_WORKSPACE_CHOICES: usize = 10;
const MAX_RECENT_WORKSPACES: usize = 12;

impl App {
    /// Advance the launcher to its workspace step and populate the first list.
    pub(super) fn open_harness_workspace_step(&mut self, edit_default: bool) {
        let default = self
            .harness_picker
            .as_ref()
            .map(|picker| picker.cwd.clone())
            .unwrap_or_default();
        if let Some(picker) = &mut self.harness_picker {
            picker.step = HarnessPickerStep::Workspace;
            picker.workspace_query = if edit_default { default } else { String::new() };
            picker.workspace_index = 0;
        }
        self.refresh_harness_workspace_choices();
        self.set_status("Workspace · type to filter · Tab complete · Enter start · Esc back");
    }

    /// Recompute cached completions after the query changes.
    pub(super) fn refresh_harness_workspace_choices(&mut self) {
        let Some(picker) = &self.harness_picker else {
            return;
        };
        let query = picker.workspace_query.clone();
        let choices = self.workspace_choices(&query);
        if let Some(picker) = &mut self.harness_picker {
            picker.workspace_choices = choices;
            picker.workspace_index = picker
                .workspace_index
                .min(picker.workspace_choices.len().saturating_sub(1));
        }
    }

    /// Selected completion, falling back to valid typed input.
    pub(super) fn selected_harness_workspace(&self) -> Option<String> {
        let picker = self.harness_picker.as_ref()?;
        if let Some(choice) = picker.workspace_choices.get(picker.workspace_index) {
            return Some(choice.path.clone());
        }
        let harnesses = self.harnesses.as_ref()?;
        let resolved = harnesses.resolve_workspace(&picker.workspace_query);
        Path::new(&resolved).is_dir().then_some(resolved)
    }

    /// Make the highlighted completion the editable query.
    pub(super) fn complete_harness_workspace(&mut self) {
        let selected = self.harness_picker.as_ref().and_then(|picker| {
            picker
                .workspace_choices
                .get(picker.workspace_index)
                .map(|choice| choice.path.clone())
        });
        if let (Some(picker), Some(selected)) = (&mut self.harness_picker, selected) {
            picker.workspace_query = selected;
            picker.workspace_index = 0;
            self.refresh_harness_workspace_choices();
        }
    }

    /// Remember a successful launch newest-first, both in memory and config.
    pub(super) fn remember_harness_workspace(&mut self, workspace: &str) -> Result<(), String> {
        let recent = &mut self.loaded.config.harness.recent_workspaces;
        recent.retain(|candidate| candidate != workspace);
        recent.insert(0, workspace.to_string());
        recent.truncate(MAX_RECENT_WORKSPACES);
        let Some(path) = &self.config_path else {
            return Ok(());
        };
        medulla::config::persist_setting(
            path,
            "harness",
            "recentWorkspaces",
            toml::Value::Array(recent.iter().cloned().map(toml::Value::String).collect()),
        )
        .map_err(|error| format!("workspace history was not saved ({error})"))
    }

    /// Rank recent, configured, and filesystem-derived workspace suggestions.
    fn workspace_choices(&self, query: &str) -> Vec<WorkspaceChoice> {
        let Some(harnesses) = &self.harnesses else {
            return Vec::new();
        };
        let base = Path::new(&harnesses.workspace);
        let resolved_query = harnesses.resolve_workspace(query);
        let mut known = Vec::new();
        for path in &self.loaded.config.harness.recent_workspaces {
            known.push((absolute(path, base), "recent"));
        }
        known.push((harnesses.workspace.clone(), "default"));
        if !self.loaded.config.host.workspace.trim().is_empty() {
            known.push((
                absolute(&self.loaded.config.host.workspace, base),
                "registered",
            ));
        }
        for path in &self.loaded.config.host.workspaces {
            known.push((absolute(path, base), "registered"));
        }
        for host in &self.loaded.config.hosts {
            if !host.workspace.trim().is_empty() {
                known.push((absolute(&host.workspace, base), "registered"));
            }
            for path in &host.workspaces {
                known.push((absolute(path, base), "registered"));
            }
        }

        let mut ranked = known
            .into_iter()
            .filter(|(path, _)| Path::new(path).is_dir())
            .filter_map(|(path, source)| {
                match_score(&path, query).map(|score| (score, WorkspaceChoice { path, source }))
            })
            .collect::<Vec<_>>();

        if !query.trim().is_empty() {
            ranked.extend(
                folder_completions(&resolved_query)
                    .into_iter()
                    .map(|(score, path)| {
                        (
                            score,
                            WorkspaceChoice {
                                path,
                                source: "folder",
                            },
                        )
                    }),
            );
        }
        ranked.sort_by(|(left_score, left), (right_score, right)| {
            left_score
                .cmp(right_score)
                .then_with(|| left.path.cmp(&right.path))
        });

        let mut seen = HashSet::new();
        ranked
            .into_iter()
            .map(|(_, choice)| choice)
            .filter(|choice| seen.insert(choice.path.clone()))
            .take(MAX_WORKSPACE_CHOICES)
            .collect()
    }
}

/// Make a configured path absolute against the local host workspace.
fn absolute(path: &str, base: &Path) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_string_lossy().into_owned()
    } else {
        base.join(path).to_string_lossy().into_owned()
    }
}

/// Rank an existing path against the query; lower scores are better.
fn match_score(path: &str, query: &str) -> Option<usize> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    let path_lower = path.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    if path_lower == query_lower {
        return Some(0);
    }
    if path_lower.starts_with(&query_lower) {
        return Some(1);
    }
    let name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    fuzzy_subsequence_score(&name, &query_lower)
        .or_else(|| fuzzy_subsequence_score(&path_lower, &query_lower))
        .map(|score| score + 10)
}

/// Complete only immediate child directories, keeping filesystem work bounded.
pub(super) fn folder_completions(query: &str) -> Vec<(usize, String)> {
    let path = Path::new(query);
    let (parent, needle) = if query.ends_with(std::path::MAIN_SEPARATOR) {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| Path::new(".")),
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        )
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let score =
                fuzzy_subsequence_score(&name.to_ascii_lowercase(), &needle.to_ascii_lowercase())?;
            Some((score, entry.path().to_string_lossy().into_owned()))
        })
        .collect()
}

/// Subsequence matcher that rewards prefixes and tightly grouped characters.
pub(super) fn fuzzy_subsequence_score(candidate: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    if candidate.starts_with(query) {
        return Some(candidate.len().saturating_sub(query.len()));
    }
    let mut score = 0;
    let mut position = 0;
    for needle in query.chars() {
        let relative = candidate.get(position..)?.find(needle)?;
        score += relative + 1;
        position += relative + needle.len_utf8();
    }
    Some(score + candidate.len().saturating_sub(query.len()))
}
