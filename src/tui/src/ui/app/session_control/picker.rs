//! Starting local harness sessions and presenting the harness-type picker.
//!
//! Workspace completion and key routing remain in the parent session-control
//! module because they share the picker state with handback input handling.

use medulla::protocol::HarnessProvider;

use crate::ui::harness_pane::HarnessChoice;

use super::super::types::{tab_pos, AgentPicker, AgentPickerStep, App, PickerPurpose};

impl App {
    /// Open the "start a session" picker, or spawn directly when the command
    /// already named a harness type.
    ///
    /// `/session` with no harness type opens the picker rather than guessing:
    /// starting the wrong CLI in the operator's workspace is not something they
    /// find out about until it has already done something.
    pub(super) fn start_session_command(&mut self, provider: Option<&str>, path: Option<&str>) {
        let Some(harnesses) = self.local_sessions.clone() else {
            self.set_status("This device is not hosting, so it has no sessions to start");
            return;
        };
        match provider.and_then(HarnessProvider::from_wire) {
            Some(provider) => {
                let cwd = path.unwrap_or("").to_string();
                self.spawn_session(HarnessChoice::native(provider), &cwd);
            }
            None => {
                let choices = harnesses.choices();
                if choices.is_empty() {
                    self.set_status("No harness CLIs found on this device");
                    return;
                }
                self.agent_picker = Some(AgentPicker {
                    purpose: PickerPurpose::Spawn,
                    choices,
                    index: 0,
                    step: AgentPickerStep::Harness,
                    cwd: path
                        .map(str::to_string)
                        .unwrap_or_else(|| harnesses.workspace.clone()),
                    workspace_query: String::new(),
                    workspace_choices: Vec::new(),
                    workspace_index: 0,
                    workspace_picked: false,
                });
            }
        }
    }

    /// Open the picker from the keyboard shortcut.
    pub(crate) fn open_session_picker(&mut self) {
        self.start_session_command(None, None);
    }

    /// Start a session the operator owns and move the cursor onto it.
    ///
    /// Always unmanaged, and not as a default the operator can override: a
    /// session started by hand is one somebody intends to type into, and the
    /// orchestrator starts its own managed without being asked. Spawning one
    /// into dispatch would mean the very next thing the operator does — press
    /// Enter on the row they just created — is a request to take it back off
    /// the orchestrator it was handed to a keystroke earlier.
    ///
    /// Selecting the new row matters more than it sounds: a session that
    /// appears somewhere below the fold, with the pane still showing whatever
    /// was selected before, reads as "nothing happened".
    pub(super) fn spawn_session(&mut self, choice: HarnessChoice, cwd: &str) {
        let Some(harnesses) = self.local_sessions.clone() else {
            self.set_status("This device is not hosting, so it has no sessions to start");
            return;
        };
        let skip = self.harness_skip_permissions;
        let workspace = harnesses.resolve_workspace(cwd);
        match harnesses.open_unmanaged(&choice, &workspace, skip) {
            Ok(id) => {
                self.tab_index = tab_pos("Agents");
                self.select_session_row(&id);
                // "unmanaged" and not a friendlier synonym: it is the word the
                // rail badge uses for the same session, and a status line that
                // renamed the state would leave the operator matching two
                // vocabularies for one fact.
                let mut status = format!(
                    "Started {} · unmanaged, the orchestrator will not use it",
                    choice.display_name(),
                );
                if let Err(error) = self.remember_harness_workspace(&workspace) {
                    status.push_str(&format!(" · {error}"));
                }
                self.set_status(status);
                // The quick path always leaves a declared agent behind if the
                // operator wants one: a session in a directory nothing declares
                // is a real thing running that the rail can only list loose.
                self.offer_agent_declaration(choice.id(), &workspace);
            }
            // Surfaced, never swallowed: a spawn that fails silently leaves the
            // operator waiting for a pane that is never coming.
            Err(err) => {
                self.set_status(format!("Could not start {}: {err}", choice.display_name()))
            }
        }
    }
}
