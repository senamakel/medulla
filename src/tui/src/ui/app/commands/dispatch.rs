//! Task, prompt, and clipboard command dispatch.

use crate::ui::agents::TaskState;
use crate::ui::clipboard::{copy_for_operator, current_platform};

use super::super::types::{tab_pos, App, Cmd, Prompt, PromptKind, SETTINGS_SUBPAGES};

impl App {
    /// The task under the Sessions rail cursor, when a `Sub` (task) row is selected.
    ///
    /// Indexes the rail's rows, which is what `rail_index` counts — the lane
    /// list alone is shorter than the rail and reading it here would answer for
    /// whichever row happened to share the offset.
    pub(in crate::ui::app) fn selected_agent_task(&self) -> Option<TaskState> {
        let lanes = self.lanes();
        let rows = self.rail_rows_in(&lanes);
        rows.get(self.rail_cursor_in(&rows, &lanes))
            .and_then(|row| row.task())
            .cloned()
    }

    /// Request cancellation of the selected running task, or note why it cannot.
    ///
    /// A dispatched task is cancelled on the hub's own runner, which admitted
    /// it: the runner sends the abort frame to the worker and settles the task.
    pub(in crate::ui::app) fn cancel_selected_task(&mut self) {
        match self.selected_agent_task() {
            Some(t) => {
                let (_, task) = crate::ui::agents::parse_task_key(&t.task_id);
                match medulla::hub::hub_in(&self.hub) {
                    Some(hub) => {
                        hub.task_runner().abort_task(task);
                        self.set_status(format!("Cancel requested · {task}"));
                    }
                    None => self.set_status("No hub is running, so nothing can be cancelled"),
                }
            }
            None => self.set_status("Select a running task (↑↓) to cancel with X"),
        }
    }

    /// Submit the open inline prompt, producing the follow-up command (if any) and
    /// closing the overlay.
    pub(in crate::ui::app) fn submit_prompt(&mut self) -> Option<Cmd> {
        let p = self.prompt.take()?;
        let text = p.draft.text.trim().to_string();
        match p.kind {
            PromptKind::ChangesBaseline => {
                self.finish_change_baseline(
                    &text,
                    super::super::changes::types::BaselineSource::Manual,
                );
                None
            }
            PromptKind::ChangesComment { path, anchor } => {
                self.submit_changes_comment(&path, anchor, &text);
                None
            }
            PromptKind::FavoriteWorkspaceAdd(workspace) => {
                self.save_favorite_workspace(&text, &workspace);
                None
            }
            PromptKind::RejectProposal {
                workflow,
                proposal_id,
            } => {
                if text.is_empty() {
                    self.set_status("Proposal rejection cancelled (empty)");
                    return None;
                }
                self.set_status("Declining the proposed change…");
                Some(Cmd::RejectProposal {
                    workflow,
                    proposal_id,
                    reason: text,
                })
            }
            PromptKind::HostField { form, field } => self.submit_host_field(*form, field, text),
            PromptKind::FeedbackComment { id } => {
                if text.is_empty() {
                    self.set_status("Comment cancelled (empty)");
                    return None;
                }
                self.set_status("Posting comment…");
                Some(Cmd::CommentFeedback { id, body: text })
            }
            // Step one captures the title and re-opens the prompt for the body;
            // nothing is sent until step two.
            PromptKind::FeedbackTitle { kind } => {
                if text.is_empty() {
                    self.set_status("New feedback cancelled (empty title)");
                    return None;
                }
                self.open_feedback_body(kind, text);
                None
            }
            PromptKind::FeedbackBody { kind, title } => {
                if text.is_empty() {
                    self.set_status("New feedback cancelled (empty description)");
                    return None;
                }
                self.set_status("Submitting feedback…");
                Some(Cmd::SubmitFeedback {
                    kind,
                    title,
                    body: text,
                })
            }
            #[cfg(feature = "workflows")]
            PromptKind::WorkflowInput {
                workflow_id,
                dry_run,
                mut remaining,
                mut collected,
            } => {
                // `remaining` is never empty: a prompt is only opened with a
                // field to ask about, and the last submission dispatches rather
                // than opening another.
                let field = remaining.remove(0);
                let value = match coerce_workflow_input(&field, &text) {
                    Ok(value) => value,
                    Err(message) => {
                        // Re-ask the same field rather than dropping the whole
                        // set: a mistyped number should cost one retype, not
                        // every value collected so far.
                        self.set_status(message);
                        remaining.insert(0, field);
                        self.open_workflow_input_prompt(workflow_id, dry_run, remaining, collected);
                        return None;
                    }
                };
                collected.insert(field.name.clone(), value);

                if !remaining.is_empty() {
                    self.open_workflow_input_prompt(workflow_id, dry_run, remaining, collected);
                    return None;
                }
                let cmd = if dry_run {
                    self.set_status("Simulating…");
                    Cmd::DryRunWorkflow {
                        id: workflow_id,
                        inputs: collected,
                    }
                } else {
                    self.set_status("Running…");
                    Cmd::RunWorkflow {
                        id: workflow_id,
                        inputs: collected,
                    }
                };
                self.scope_workflow_cmd(Some(cmd))
            }
        }
    }

    /// Open the prompt for `remaining`'s first field, carrying the rest and
    /// what has been collected so far.
    ///
    /// Pre-filled with the field's declared default so the common answer is one
    /// keypress, and titled with the field name so a chain of several prompts
    /// says which one is on screen.
    #[cfg(feature = "workflows")]
    pub(in crate::ui::app) fn open_workflow_input_prompt(
        &mut self,
        workflow_id: String,
        dry_run: bool,
        remaining: Vec<medulla::workflows::WorkflowInput>,
        collected: serde_json::Map<String, serde_json::Value>,
    ) {
        let Some(field) = remaining.first() else {
            return;
        };
        let title = match (&field.description, field.required) {
            (Some(description), _) if !description.is_empty() => {
                format!("{}: {description}", field.name)
            }
            (_, true) => format!("{} (required)", field.name),
            (_, false) => field.name.clone(),
        };
        let prefill = field
            .default
            .as_ref()
            .map(render_workflow_input_default)
            .unwrap_or_default();
        self.prompt = Some(Prompt::with_text(
            PromptKind::WorkflowInput {
                workflow_id,
                dry_run,
                remaining,
                collected,
            },
            title,
            prefill,
        ));
    }

    /// Copy one short line — a command, an address — to the clipboard, naming it
    /// in the status line.
    ///
    /// It reports *what* was copied rather than a size, because for one line
    /// the size says nothing. And it goes to the terminal first
    /// ([`copy_for_operator`](medulla::clipboard::copy_for_operator)): this
    /// `medulla` may itself be running over SSH, and a short line is exactly
    /// what an operator then pastes somewhere else. The pointer selection keeps
    /// the local-writer-first path, since terminals cap how much an OSC 52
    /// escape may carry and a long block would be truncated or dropped outright.
    pub(in crate::ui::app) fn copy_line(&mut self, what: &str, text: &str) {
        if let Some(sink) = &self.copy_capture {
            sink.lock().expect("copy sink").push(text.to_string());
            self.set_status(format!("Copied {what} (captured)"));
            return;
        }
        let report = copy_for_operator(text, current_platform(), |osc| {
            use std::io::Write;
            let _ = std::io::stdout().write_all(osc.as_bytes());
            let _ = std::io::stdout().flush();
        });
        self.set_status(if report.confirmed() {
            format!("Copied {what} → clipboard ({})", report.describe())
        } else {
            format!("Sent {what} → terminal (OSC 52); check your clipboard")
        });
    }

    /// Land on the Settings tab at subpage `index`, returning its lazy-load
    /// command (Usage and Context each fetch on entry).
    pub(in crate::ui::app) fn set_settings_subpage(&mut self, index: usize) -> Option<Cmd> {
        self.tab_index = tab_pos("Settings");
        self.settings_index = index.min(SETTINGS_SUBPAGES.len() - 1);
        // An armed logout must not survive a jump to another subpage.
        self.disarm_logout();
        // A jump lands on the nav, not inside the new page: the digit keys are a
        // way to move *between* subpages, so leaving focus in the content pane
        // would strand the next arrow key on whatever page you just left.
        self.settings_focused = false;
        self.tab_enter_cmd()
    }
}

/// Turn what an operator typed into a value of the input's declared type.
///
/// A prompt yields a string because a terminal line is one. The declaration is
/// what says whether `3` means the number three or the string `"3"`, so the
/// coercion is driven by it rather than by guessing from the text — otherwise a
/// version input typed as `1.0` would silently become a float.
///
/// An empty line means "leave it unset": the declared default applies, or the
/// input resolves null if it is optional. A *required* input with no default
/// cannot be left unset, and says so rather than sending an empty string that
/// would pass the type check and fail the operator's intent.
#[cfg(feature = "workflows")]
fn coerce_workflow_input(
    field: &medulla::workflows::WorkflowInput,
    text: &str,
) -> Result<serde_json::Value, String> {
    use medulla::workflows::InputType;

    if text.is_empty() {
        return match (&field.default, field.required) {
            (Some(default), _) => Ok(default.clone()),
            (None, false) => Ok(serde_json::Value::Null),
            (None, true) => Err(format!("{} is required", field.name)),
        };
    }

    match field.ty {
        InputType::String => Ok(serde_json::Value::String(text.to_string())),
        InputType::Number => text
            .parse::<serde_json::Number>()
            .map(serde_json::Value::Number)
            .map_err(|_| format!("{} expects a number, got {text:?}", field.name)),
        InputType::Boolean => text
            .parse::<bool>()
            .map(serde_json::Value::Bool)
            .map_err(|_| format!("{} expects true or false, got {text:?}", field.name)),
        InputType::Json => {
            serde_json::from_str(text).map_err(|err| format!("{} expects JSON: {err}", field.name))
        }
    }
}

/// Render a declared default as the text to pre-fill its prompt with.
///
/// A string default is shown unquoted — the operator is editing the value, not
/// its JSON encoding, and leaving the quotes in would make accepting the
/// default produce a differently-quoted string.
#[cfg(feature = "workflows")]
fn render_workflow_input_default(default: &serde_json::Value) -> String {
    match default {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
