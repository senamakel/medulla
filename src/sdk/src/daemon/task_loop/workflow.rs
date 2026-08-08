//! Running an installed workflow in answer to a task frame.
//!
//! An ordinary task frame carries an instruction and this worker hands it to a
//! harness. A frame naming a `workflow` carries an *id*, and this worker runs
//! the saved graph instead — dispatching each of its `agent` nodes to its own
//! harness, in the order and with the parallelism the graph declares.
//!
//! That is the whole of "the orchestrator can execute workflows": one extra
//! field on a frame it already sends, over the transport it already uses. The
//! admission check, the ack, and the reply are the same ones an ordinary task
//! gets, so an orchestrator that knows nothing about workflows still sees a
//! task it dispatched and a task that answered.

use std::sync::Arc;

use async_trait::async_trait;

use crate::flow_engine::caps::dispatch::HarnessDispatch;
use crate::flow_engine::{folding_sink, CapabilitySettings, HostServices};
use crate::hub::{RunError, TaskOutcome, TaskRequest};
use crate::protocol::{TaskFrame, TaskFrameKind, TokenUsage, WorkflowAdvert, WorkflowInputAdvert};
// `trigger_input` is shared with the cloud plane's adapter
// ([`crate::workflows::bridge`]) rather than defined twice: a frame's text must
// become the same trigger payload whether it arrived over the host link or over the
// backend socket, and two copies of that rule would eventually disagree.
use crate::workflows::bridge::trigger_input;
use crate::workflows::evolve::{EvolveConfig, EvolveSession, EvolveTrigger};
use crate::workflows::{
    run_workflow_versioned, FileWorkflowStore, RunContext, RunStatus, StoreWorkflowResolver,
    WorkflowStore,
};

use super::super::providers::{Abort, RunTaskOptions};
use super::super::types::{DaemonRuntime, FrameAttachments, CAPACITY_REJECTION_PREFIX};

/// Dispatch a workflow's `agent` nodes through this daemon's own executor.
///
/// A worker running a workflow already *is* a harness host, so a node's
/// instruction goes straight to the executor rather than back out over a bridge
/// to itself. The node's `agent_ref` names a provider hint when it matches one
/// this worker offers; otherwise the worker's default runs it.
pub(in crate::daemon) struct RuntimeDispatch {
    runtime: DaemonRuntime,
    /// The authenticated sender the workflow is being run for, so nodes inherit
    /// the same conversation attribution an ordinary task would get.
    conversation: String,
}

impl RuntimeDispatch {
    /// Build a dispatch attributed to one authenticated sender.
    pub(in crate::daemon) fn new(runtime: DaemonRuntime, conversation: String) -> Self {
        Self {
            runtime,
            conversation,
        }
    }

    /// The custom harness preset `request` names, resolved against this host.
    ///
    /// A workflow node reaches a harness through the same presets an ordinary
    /// task frame does, so this mirrors `handle_task`'s lookup rather than
    /// inventing a second rule: an explicitly named preset that this host has
    /// not configured is an error, and a request that states no preference at
    /// all inherits the operator's default preset when one is usable.
    ///
    /// Resolving it is what makes a preset more than a model string. The preset
    /// carries the endpoint, the API key name, and the harness's own knobs, and
    /// a dispatch that reads only [`TaskRequest::model`] sends a routed model
    /// slug to the harness's *default* account — which fails at the provider,
    /// far from the configuration that caused it.
    ///
    /// # Errors
    ///
    /// Returns [`RunError::Worker`] when `request` names a preset this host has
    /// no configuration for. Refused rather than silently downgraded to the
    /// default harness: a node that asked for a specific model and credentials
    /// must not quietly run on someone else's.
    fn preset(
        &self,
        request: &TaskRequest,
    ) -> Result<Option<crate::config::CustomHarnessConfig>, RunError> {
        let config = &self.runtime.inner.config;
        match request.custom_harness.as_deref() {
            Some(id) => config
                .custom_harnesses
                .iter()
                .find(|harness| harness.id == id)
                .cloned()
                .map(Some)
                .ok_or_else(|| {
                    RunError::Worker(format!(
                        "custom harness \"{id}\" is not configured on this host"
                    ))
                }),
            // Only when the node stated no preference of its own at all: a node
            // that named a plain provider asked for that provider, not for
            // whatever preset the operator happens to have marked default.
            None if request.provider.is_none() => Ok(config
                .custom_harnesses
                .iter()
                .find(|harness| harness.default && harness.key_present(&config.env))
                .cloned()),
            None => Ok(None),
        }
    }

    /// The provider and transport `request` will actually run on.
    ///
    /// An address hint may fall back for portability, but a named preset may
    /// not: its endpoint, credentials, and model only make sense with its
    /// base harness. The two callers that need the resolved pair — the
    /// dispatch itself and the run inspector's harness label — therefore share
    /// this rule.
    ///
    /// # Errors
    ///
    /// Returns [`RunError::Worker`] when a named preset's base provider is not
    /// offered by this daemon. Falling through to another provider would pair
    /// the preset's routing configuration with the wrong harness.
    fn resolve(
        &self,
        request: &TaskRequest,
        preset: Option<&crate::config::CustomHarnessConfig>,
    ) -> Result<
        (
            crate::protocol::HarnessProvider,
            crate::protocol::HarnessTransport,
        ),
        RunError,
    > {
        let inner = &self.runtime.inner;
        // A node that named the embedded core gets it, whether or not this
        // worker "offers" it. `config.providers` is the list of coding CLIs
        // found on PATH (see
        // [`crate::daemon::providers::detect_providers`]), and OpenHuman is
        // never on it — it has no binary to find. Falling through would send a
        // node that explicitly asked for the operator's own core to a coding
        // CLI instead, which is the one substitution that changes what the node
        // is for.
        //
        // Below the preset check rather than above it: a preset is a complete
        // description of one harness, so a node that named one has named
        // something more specific than a bare provider. In practice the two
        // cannot both be set — naming a preset leaves `provider` empty — and
        // this ordering is what keeps that true if they ever could.
        if preset.is_none()
            && (request.provider == Some(crate::protocol::HarnessProvider::Openhuman)
                || request.worker_address == crate::protocol::HarnessProvider::Openhuman.as_str())
        {
            return Ok((
                crate::protocol::HarnessProvider::Openhuman,
                crate::protocol::HarnessTransport::Cli,
            ));
        }
        // A preset outranks the address hint: it is a complete description of
        // one harness — binary, endpoint, credentials, model — and running it on
        // any other provider would pair its model with an account that cannot
        // serve it. A node may name a provider through its `agent_ref`;
        // anything this worker does not offer falls back to the default rather
        // than failing.
        let provider = preset
            .map(|harness| {
                self.runtime
                    .select_provider(Some(harness.base_harness))
                    .ok_or_else(|| unavailable_provider_error(inner, Some(harness.base_harness)))
            })
            .transpose()?
            .or_else(|| {
                crate::protocol::HarnessProvider::from_wire(&request.worker_address)
                    .filter(|p| inner.config.providers.contains(p))
            })
            .or_else(|| self.runtime.select_provider(request.provider))
            .unwrap_or(inner.config.default_provider);
        // Dropped when the provider fell back, because a transport the chosen
        // provider cannot speak is not a transport at all.
        let transport = request
            .transport
            .filter(|transport| transport.supported_by(provider))
            .unwrap_or_default();
        Ok((provider, transport))
    }
}

#[async_trait]
impl HarnessDispatch for RuntimeDispatch {
    async fn dispatch(&self, request: TaskRequest) -> Result<TaskOutcome, RunError> {
        let inner = &self.runtime.inner;
        let preset = self.preset(&request)?;
        let (provider, transport) = self.resolve(&request, preset.as_ref())?;

        // The preset's non-secret knobs ride down in the environment, which is
        // what every spawn seam hands the child unchanged — see
        // `crate::codex_overrides`, which reads them back there. Without this a
        // `codexOverrides` preset reaches Codex as a bare `-m <slug>` and Codex
        // asks its own default account for a model that account cannot serve.
        let mut run_env = inner.config.env.clone();
        if let Some(harness) = &preset {
            run_env.extend(harness.harness_env());
        }

        // Shared with the `on_event` callback below, which the executor owns
        // for the life of the run and drops before returning. A mutex rather
        // than a channel because the collector *is* the bound: a channel would
        // buffer everything a chatty node emits before anything applied a cap.
        let transcript = Arc::new(std::sync::Mutex::new(
            crate::harness_transcript::TranscriptCollector::new(),
        ));

        let options = RunTaskOptions {
            conversation: self.conversation.clone(),
            // A workflow node is discrete work, like the task frame that
            // started the graph — nodes share a conversation for attribution,
            // not a harness. Two nodes of one graph running in the same session
            // would let a later node read an earlier one's prompt as context.
            session_class: crate::sessions::SessionClass::Bounded,
            resume_session_id: None,
            workspace_context: Default::default(),
            provider,
            transport,
            prompt: request.instruction,
            cwd: inner.config.workspace.clone(),
            // Withheld, not merely unset: a node's harness is a *step* of a
            // graph that is already running. `workflow_run` would let it start
            // another one outside the loop bound, the approval gates, and the
            // concurrency budget the engine applies to its own nodes, and the
            // `fleet_*` verbs would let it dispatch into the very worker pool
            // this run is competing for. See [`crate::harness_tools`].
            //
            // Note what this replaces, and what follows from it. The ordinary
            // task path calls `with_tool_mode_at_depth`, which forces the ACP
            // transport whenever tools are wanted — ACP being the only way to
            // hand a harness an MCP server. Wanting no tools removes that
            // reason, so a node lands on the plain CLI spawn instead, and
            // `harness_hooks::launch_args` installs the operator's hooks onto
            // its argv for *every* provider. That is a real gain rather than a
            // neutral swap: ACP delivers hooks only to Claude, through its
            // session metadata, and Codex's ACP app-server still runs none
            // (see `crate::harness_hooks::acp`).
            //
            // `run_env`, not `config.env`: a node that selected a custom preset
            // needs that preset's non-secret knobs, which the spawn seam reads
            // back out of the environment.
            env: {
                let mut env = run_env;
                crate::harness_tools::withhold(&mut env);
                env.insert(
                    crate::control_socket::FLEET_DEPTH_ENV.to_string(),
                    request.fleet_depth.to_string(),
                );
                env
            },
            timeout_ms: inner.config.task_timeout_ms,
            // The preset's own model sits between the node's hint and this
            // daemon's pin: a node that selected a preset without naming a model
            // asked for that preset's model, not for whatever this host runs
            // when nobody states a preference.
            model: request
                .model
                .or_else(|| preset.as_ref().map(|harness| harness.model.clone()))
                .or_else(|| inner.config.model.clone()),
            agent: inner.config.agent.clone(),
            extra_args: inner.config.extra_args.clone(),
            skip_permissions: inner.config.skip_permissions,
            // The preset's endpoint and API-key name. The key itself is
            // resolved by name at the spawn seam, never inlined here.
            router: preset
                .as_ref()
                .map(crate::config::CustomHarnessConfig::router)
                .or_else(|| inner.config.router.clone()),
            attribution: inner.config.attribution,
            hooks: inner.config.hooks.clone(),
            abort: Abort::new(),
            // Collected, not forwarded. The run observer still reports progress
            // per node — nothing here emits a second status stream, which is
            // what the previous `None` was protecting against — but the events
            // are folded into a transcript that settles onto the run record.
            //
            // A node runs headless with nobody watching, so the reply used to
            // be all that survived it: the run view could say a step succeeded
            // and took four minutes without saying what happened in them. The
            // collector is bounded on the way in, so a chatty node costs a
            // fixed amount of memory rather than however much it emits.
            on_event: {
                let transcript = transcript.clone();
                Some(Box::new(move |event| {
                    if let Ok(mut collector) = transcript.lock() {
                        collector.observe(event);
                    }
                }))
            },
            on_stdin: None,
            on_session: None,
            on_workspace_context: None,
        };

        // Workflow nodes and detached evolution reviews run outside the inbound
        // task handler, but they are still harness sessions on this host. Share
        // its semaphore so a burst of failed workflows cannot exceed the
        // operator's configured concurrency.
        let _permit = inner
            .slots
            .acquire()
            .await
            .expect("semaphore is never closed");
        let result = (inner.run_task)(options).await.map_err(RunError::Worker)?;
        // The executor has returned, so it has dropped its `on_event` callback
        // and this is the only remaining handle — but a poisoned lock is still
        // possible (a panic inside the callback), and losing the transcript is
        // not a reason to fail a node that otherwise succeeded.
        let transcript = Arc::try_unwrap(transcript)
            .ok()
            .and_then(|lock| lock.into_inner().ok())
            .map(crate::harness_transcript::TranscriptCollector::finish)
            .unwrap_or_default();
        Ok(TaskOutcome {
            reply: result.reply,
            usage: result.usage.unwrap_or(TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            }),
            harness: Some(provider),
            session_id: None,
            transcript,
        })
    }

    /// The flavor this worker will really run the node on.
    ///
    /// `None` for a request naming a custom harness: this dispatch resolves
    /// providers, not custom harnesses, so the requested name remains the
    /// closest thing to the truth and overriding it would lose information.
    fn effective_harness(&self, request: &TaskRequest) -> Option<String> {
        if request.custom_harness.is_some() {
            return None;
        }
        // A preset can still be resolved here — the operator's default one, for
        // a request that named nothing. An unconfigured preset cannot reach this
        // point (the branch above returned), so the error case is unreachable
        // and reported as "no preset" rather than widening this signature.
        let preset = self.preset(request).ok().flatten();
        let (provider, transport) = self.resolve(request, preset.as_ref()).ok()?;
        Some(provider.flavor_name(transport).to_string())
    }
}

/// Match the task-frame handler's error when a requested provider is absent.
fn unavailable_provider_error(
    inner: &super::super::types::Inner,
    requested_provider: Option<crate::protocol::HarnessProvider>,
) -> RunError {
    let offered = inner
        .config
        .providers
        .iter()
        .map(|provider| provider.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let offered = if offered.is_empty() {
        "(none)".to_string()
    } else {
        offered
    };
    let requested = requested_provider
        .map(|provider| format!(" for requested \"{}\"", provider.as_str()))
        .unwrap_or_default();
    RunError::Worker(format!(
        "no available provider{requested}; daemon offers: {offered}"
    ))
}

impl DaemonRuntime {
    /// The workflows this worker has installed, for its capability advert.
    ///
    /// Best effort: a store that cannot be read advertises nothing rather than
    /// failing the probe, because a worker with an unreadable workflow directory
    /// is still a perfectly good worker for ordinary tasks.
    pub(super) fn installed_workflows(&self) -> Vec<WorkflowAdvert> {
        // A host with workflows off advertises none: an orchestrator should not
        // be told about work it will be refused.
        if !self.workflow_settings().enabled {
            return Vec::new();
        }
        let store = self.workflow_store();
        store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|summary| summary.enabled)
            .filter_map(|summary| {
                let record = store.get(&summary.id).ok().flatten()?;
                Some(WorkflowAdvert {
                    id: summary.id,
                    name: summary.name,
                    description: summary.description,
                    node_count: summary.node_count,
                    fingerprint: crate::workflows::record_fingerprint(&record),
                    inputs: summary
                        .inputs
                        .into_iter()
                        .map(|input| WorkflowInputAdvert {
                            name: input.name,
                            ty: input.ty.as_str().to_string(),
                            description: input.description.unwrap_or_default(),
                            required: input.required,
                            default: input.default,
                        })
                        .collect(),
                })
            })
            .collect()
    }

    /// The workflow store for this worker: its configured workspace, layered
    /// over the user-global directory the way every other workflow surface
    /// resolves it.
    fn workflow_store(&self) -> Arc<dyn WorkflowStore> {
        let cwd = std::path::Path::new(&self.inner.config.workspace);
        Arc::new(FileWorkflowStore::discover(&self.inner.config.env, cwd))
    }

    /// Run the workflow a `task` frame named, replying with its outcome.
    pub(super) async fn handle_workflow_task(&self, from: String, frame: TaskFrame, id: String) {
        let correlation = frame.correlation_id.clone();

        let mut settings = CapabilitySettings::clone(&self.workflow_settings());
        settings.fleet_depth = frame.fleet_depth;
        let settings = Arc::new(settings);
        if !settings.enabled {
            self.reply(
                &from,
                TaskFrameKind::Error,
                &frame.task_id,
                "workflows are disabled on this worker (workflows.enabled = false)",
                correlation.as_deref(),
                None,
            )
            .await;
            return;
        }

        // Held for the rest of the call, including the validation rejections
        // below: releasing it is the guard's job, never a hand-written
        // decrement that an unwind can skip.
        let Some(admission) = self.admit() else {
            self.reply(
                &from,
                TaskFrameKind::Error,
                &frame.task_id,
                &format!(
                    "{CAPACITY_REJECTION_PREFIX} ({} pending tasks); retry later",
                    self.inner.config.max_pending
                ),
                correlation.as_deref(),
                None,
            )
            .await;
            return;
        };

        let store = self.workflow_store();
        if store.get(&id).ok().flatten().is_none() {
            // Naming what *is* installed turns a failed dispatch into something
            // the orchestrator can correct on its next attempt.
            let known: Vec<String> = store
                .list()
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.id)
                .collect();
            let known = if known.is_empty() {
                "(none installed)".to_string()
            } else {
                known.join(", ")
            };
            self.reply(
                &from,
                TaskFrameKind::Error,
                &frame.task_id,
                &format!("no workflow '{id}' on this worker; installed: {known}"),
                correlation.as_deref(),
                None,
            )
            .await;
            return;
        }

        // A resent frame (a lost ack) or a sender reusing an active id must not
        // start the graph twice: both copies would run every node's side
        // effects and race to overwrite the same run record.
        if crate::workflows::run::is_running(&frame.task_id) {
            self.reply(
                &from,
                TaskFrameKind::Error,
                &frame.task_id,
                &format!("workflow {} is already running", frame.task_id),
                correlation.as_deref(),
                None,
            )
            .await;
            return;
        }

        self.reply(
            &from,
            TaskFrameKind::Ack,
            &frame.task_id,
            "workflow accepted",
            correlation.as_deref(),
            None,
        )
        .await;
        self.log(&format!("workflow {} → {id}", frame.task_id));

        let (sink, fold) = folding_sink();
        // Kept past the move into the resolver: a failed run gets a review, and
        // the review reads the same store the run wrote to.
        let evolve_store = store.clone();
        let max_loop_iterations = settings.max_loop_iterations;
        let context = RunContext {
            // Runs inline, so claiming at the top of the run is early enough.
            claim: None,
            store: store.clone(),
            settings,
            services: HostServices {
                dispatch: Arc::new(RuntimeDispatch::new(self.clone(), from.clone())),
                node_progress: None,
                resolver: Arc::new(StoreWorkflowResolver::new(store, max_loop_iterations)),
                http_credentials: Default::default(),
            },
            sink,
            step_snapshot: None,
            // A fleet dispatch: this run exists because a peer sent a task
            // frame, and the address it came from is the only thing that can
            // say which one.
            origin: Some(
                crate::workflows::RunOrigin::of_kind("dispatch")
                    .labelled(format!("task {} from {from}", frame.task_id)),
            ),
        };

        // The frame's task id becomes the run id, so the orchestrator's existing
        // `abort` for that task is exactly what cancels the run.
        let Some(fingerprint) = frame.workflow_fingerprint.clone() else {
            self.reply(
                &from,
                TaskFrameKind::Error,
                &frame.task_id,
                "workflow dispatch is missing its definition fingerprint; refresh the worker catalog",
                correlation.as_deref(),
                None,
            )
            .await;
            return;
        };
        let outcome = run_workflow_versioned(
            context,
            &id,
            &frame.task_id,
            trigger_input(&frame.text),
            frame.workflow_inputs,
            &fingerprint,
        )
        .await;

        let work = fold.lock().ok().map(|fold| fold.snapshot().clone());
        // No session id: a workflow run is a graph, not one harness session —
        // each `agent` node opens its own. There is no single session that
        // served this task, so none is claimed.
        let attachments = FrameAttachments {
            usage: None,
            work,
            ..Default::default()
        };

        match outcome {
            Ok(record) => {
                let failed = record.status == RunStatus::Failed;
                let text = summarize(&record);
                let kind = match record.status {
                    RunStatus::Succeeded | RunStatus::PendingApproval => TaskFrameKind::Reply,
                    _ => TaskFrameKind::Error,
                };
                self.reply_with(
                    &from,
                    kind,
                    &frame.task_id,
                    &text,
                    correlation.as_deref(),
                    None,
                    attachments,
                )
                .await;
                if failed {
                    self.spawn_review(evolve_store, &id, &frame.task_id, &from);
                }
            }
            Err(err) => {
                self.reply_with(
                    &from,
                    TaskFrameKind::Error,
                    &frame.task_id,
                    &format!("workflow '{id}' failed: {err}"),
                    correlation.as_deref(),
                    None,
                    attachments,
                )
                .await;
            }
        }

        // After the terminal frame, never before: the slot this run occupied is
        // only genuinely free once the requester has been told how it ended.
        drop(admission);
    }

    /// Start a review of a workflow whose run just failed.
    ///
    /// Spawned rather than awaited, and only after the reply has gone out: a
    /// review is a whole harness turn, and the orchestrator waiting on this
    /// task should not be held for one. Nothing depends on its result here —
    /// what it produces is a note and possibly a proposal, both of which live
    /// in the store for an operator to find.
    ///
    /// The run's own failure note is *not* written here. `run_workflow` already
    /// wrote it, synchronously, so it exists whether or not this review ever
    /// starts.
    fn spawn_review(
        &self,
        store: Arc<dyn WorkflowStore>,
        workflow_id: &str,
        run_id: &str,
        from: &str,
    ) {
        let settings = self.evolve_settings();
        if !settings.enabled || !settings.auto_on_failure {
            return;
        }
        let session = EvolveSession {
            store,
            dispatch: Arc::new(RuntimeDispatch::new(self.clone(), from.to_string())),
            worker_address: self.inner.config.default_provider.as_str().to_string(),
            provider: Some(self.inner.config.default_provider),
            model: self.inner.config.model.clone(),
            // Per workflow, not per run, so the attribution of successive
            // reviews reads as one thread. It does not buy harness continuity
            // here: `RuntimeDispatch` runs every task `Bounded` and ignores
            // this field. What actually carries knowledge between reviews is
            // the journal, which is the point — a note survives a restart and a
            // resumed session does not.
            conversation: format!("evolve:{workflow_id}"),
            config: settings,
        };
        let workflow_id = workflow_id.to_string();
        let trigger = EvolveTrigger::Failure(run_id.to_string());
        // `--once` waits on the same counter as inbound tasks. Register the
        // detached review before spawning it so the daemon cannot observe an
        // idle gap and exit between the workflow reply and this turn.
        self.inner
            .inflight_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let runtime = self.clone();
        tokio::spawn(async move {
            match session.evolve(&workflow_id, trigger, None).await {
                Ok(outcome) if outcome.skipped => {
                    tracing::debug!(workflow = %workflow_id, "a review is already in flight");
                }
                Ok(outcome) => tracing::info!(
                    workflow = %workflow_id,
                    notes = outcome.notes.len(),
                    proposals = outcome.proposals.len(),
                    "reviewed a failed run",
                ),
                Err(err) => {
                    tracing::warn!(workflow = %workflow_id, "review failed: {err}")
                }
            }
            if runtime
                .inner
                .inflight_count
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
                == 1
            {
                runtime.inner.inflight_idle.notify_waiters();
            }
        });
    }

    /// This worker's review settings, failing closed when config is unreadable.
    fn evolve_settings(&self) -> EvolveConfig {
        let cwd = std::path::Path::new(&self.inner.config.workspace);
        crate::config::load_config(
            crate::config::explicit_config_from_env(&self.inner.config.env),
            &self.inner.config.env,
            cwd,
        )
        .map(|loaded| EvolveConfig::from_config(&loaded.config.workflows))
        .unwrap_or_else(|err| {
            tracing::warn!("could not reload evolution policy; reviews disabled: {err}");
            EvolveConfig {
                enabled: false,
                auto_on_failure: false,
                ..EvolveConfig::default()
            }
        })
    }

    /// Capability settings for workflows run on this worker.
    ///
    /// Read from the operator's layered config so `workflows.enabled` and the
    /// allowlists mean the same thing here as they do on the CLI. A config that
    /// cannot be loaded fails closed, including code execution.
    fn workflow_settings(&self) -> Arc<CapabilitySettings> {
        let home = crate::home::medulla_home(&self.inner.config.env);
        let cwd = std::path::Path::new(&self.inner.config.workspace);
        let mut settings = crate::config::load_config(
            crate::config::explicit_config_from_env(&self.inner.config.env),
            &self.inner.config.env,
            cwd,
        )
        .map(|loaded| CapabilitySettings::from_config(&loaded.config.workflows, &home))
        .unwrap_or_else(|err| {
            tracing::warn!("could not reload workflow policy; code execution disabled: {err}");
            CapabilitySettings::fail_closed_at(home)
        });
        // The daemon's own workspace, which is the directory it serves tasks
        // for — the same one an `agent` node's harness session runs in.
        settings.workspace = self.inner.config.workspace.clone();
        settings.default_worker_address = self.inner.config.default_provider.as_str().to_string();
        settings.default_provider = Some(self.inner.config.default_provider);
        settings.default_model = self.inner.config.model.clone();
        Arc::new(settings)
    }
}

/// A one-line account of how a run ended, for the reply frame's text.
///
/// Delegates rather than phrasing its own: a run that is described one way in
/// the reply frame and another way in its own record is a run an operator has
/// to reconcile by hand.
fn summarize(record: &crate::workflows::RunRecord) -> String {
    if record.status == RunStatus::Failed {
        return crate::workflows::run::summarize(record);
    }
    record
        .summary
        .clone()
        .unwrap_or_else(|| crate::workflows::run::summarize(record))
}
