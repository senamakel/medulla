//! Handing a harness Medulla spawns the tools it is meant to have.
//!
//! Two transports start harnesses, and until now only one of them attached this
//! server:
//!
//! * **ACP** — Medulla is the client, so `session/new`'s `mcpServers` carries
//!   the offer ([`crate::daemon::providers::acp`]).
//! * **PTY** — the harness is started the way a person starts it, as a CLI on a
//!   pseudo-terminal. There is no `session/new` to put an offer in, so the
//!   registration has to go on the child's own argv. Nothing did that, which is
//!   why an operator sitting in a Medulla-spawned Claude Code session had no
//!   `workflow_*` or `fleet_*` tools at all: the skills told the model to call
//!   `workflow_run` and the tool was not there.
//!
//! Everything both transports agree on lives here — whether a server is worth
//! attaching, which grant the session gets, and what the child is told to run —
//! so the two doors cannot drift into serving different tool surfaces.
//!
//! # Where the secret goes
//!
//! The fleet grant is a bearer token, and two different processes must never
//! see it: another local user, and any of the harness's *other* children.
//!
//! It cannot go on argv: `/proc/<pid>/cmdline` is world-readable on Linux while
//! `/proc/<pid>/environ` is readable only by the owner, so an argv registration
//! would hand every local user this session's capability. The registration
//! itself (binary path, `mcp` argument, tool mode) holds no secret and goes on
//! argv freely.
//!
//! Nor can it go on the *harness's own environment*, the way [`Self::env`]
//! does: that environment is inherited by every subprocess the harness spawns,
//! not just the `medulla mcp` server it is meant for — a configured
//! provider-binary override, a shell command, or another project's MCP server
//! sharing that environment could redeem it against the control socket for as
//! long as the session runs. So [`ServerSpec::secret_env`] instead reaches
//! Claude Code through a `--mcp-config` *file* ([`ServerSpec::write_config_file`])
//! that only its owner can read, naming the grant in the one server entry's own
//! `env` block. Claude Code applies a server's `env` only when spawning that
//! server, so the grant reaches the one subprocess it is for and nothing else
//! the harness starts. [`revoke_session`] removes the file along with the
//! grant it names.
//!
//! # Where the file lives, and what closes it against a symlink
//!
//! The file lives under this process's own [`crate::home::medulla_home`], not
//! a shared directory like `/tmp`: that account directory already holds every
//! other piece of this account's runtime state, and unlike a world-writable
//! temp directory nothing else on the machine has a reason to write there.
//! Naming still leans on `session`'s own randomness (a fresh UUID per PTY
//! launch — see `worker::pty::launch::attach_mcp` in the `medulla-tui` crate)
//! rather than on the directory being unshared, because the mode alone is not
//! what stops a pre-planted entry — including a symlink — from being followed
//! and written through by a plain `create`. [`write_owner_only`] opens with
//! `O_CREAT | O_EXCL` (`create_new`) instead, so an existing entry at the
//! target path makes the open fail rather than get overwritten, and the
//! caller's already-safe fallback (the non-secret inline document) takes over.
//! Revoking a session removes the file the same way it revokes the grant, but
//! a process that exits without reaching that call — killed, or crashed —
//! leaves its file behind; see [`sweep_stale_config_files`] for why that is
//! survivable rather than a second leak, and [`instance_key`] for why sweeping
//! is namespaced per control-plane socket rather than shared account-wide —
//! two Medullas on one machine can legitimately share this account and bind
//! different sockets, and one restarting must never sweep the other's still-
//! live files. `session` itself is validated
//! ([`is_safe_session_component`]) before it ever reaches a path: this module
//! mints it as a UUID internally, but [`attach_cli`] and
//! [`ServerSpec::write_config_file`] are public API, and an unchecked `/` or
//! `..` in a caller-supplied session would let it name a file — or, for
//! [`revoke_session`], *delete* one — outside this directory entirely.
//!
//! # What the file does not protect against
//!
//! Everything above assumes the process `--mcp-config` registers this onto
//! *is* the verified Claude Code CLI, which only ever applies a server's `env`
//! to that server's own spawn. That process necessarily receives the file's
//! path on its own argv — that is how it knows what to read — so nothing
//! about file permissions stops it from opening the file directly instead of
//! behaving like Claude Code. A provider-binary override
//! ([`crate::protocol::env::bin_is_overridden`]) can name any
//! executable there, so [`attach_cli`] withholds the fleet grant whenever one
//! is in effect, per `AGENTS.md`'s "provider binary overrides are untrusted
//! configuration."

mod files;
mod server;

use crate::protocol::HarnessProvider;
pub use files::{config_dir_for_socket, sweep_stale_config_files};
use files::{config_file_path, write_owner_only};
pub use medulla_harness::mcp::ATTACHED_ENV;
use medulla_harness::mcp::{claude_mcp_config, codex_config_args as render_codex_config_args};
pub use server::{server_command, server_spec, ServerSpec, ORIGIN_SESSION_ENV, SERVER_COMMAND_ENV};
use std::collections::HashMap;
use std::path::PathBuf;

pub const SERVER_NAME: &str = "medulla";
pub use medulla_types::TOOL_MODE_ENV;
pub const TOOL_SCOPE_ENV: &str = "MEDULLA_WORKFLOW_SCOPE";

/// Whether workflow authoring is switched on for this host.
///
/// Resolved through the config the *parent* chose (`--config`, recorded in
/// [`crate::config::CONFIG_PATH_ENV`]) rather than rediscovered, so a policy the
/// operator set explicitly is not overridden by whatever a child's working
/// directory happens to find. An unreadable config is treated as enabled, which
/// matches [`crate::config`]'s own default.
pub fn workflows_enabled(env: &HashMap<String, String>) -> bool {
    // A launch whose workflow surface was withheld is answered before the
    // config is consulted: the host's own `workflows.enabled` says what this
    // machine permits in general, and this says what *this* launch gets. Here
    // rather than at each call site because this function is the shared policy
    // both transports resolve through — the ACP session's `mcpServers` and the
    // CLI's `--mcp-config` — and it is also what `session_grant` turns into
    // [`ToolFamilies::fleet_only`](crate::control_socket::ToolFamilies::fleet_only),
    // so the tools are absent from `tools/list` rather than refused on call.
    if crate::harness_tools::workflows_withheld(env) {
        return false;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    crate::config::load_config(crate::config::explicit_config_from_env(env), env, &cwd)
        .map(|loaded| loaded.config.workflows.enabled)
        .unwrap_or(true)
}

/// Build the capability for one session from that session's own environment.
///
/// A process serves sessions at many depths concurrently, so ambient process
/// variables cannot describe the one being created — the depth is read from the
/// environment the *task* was dispatched with and recorded in the grant, so
/// every later check consults the grant rather than an environment the harness
/// itself could rewrite.
pub fn session_grant(
    session: &str,
    task_env: &HashMap<String, String>,
    tool_mode: Option<&str>,
    workflows_enabled: bool,
    max_depth: u8,
    max_in_flight: usize,
) -> crate::control_socket::Grant {
    let depth = crate::control_socket::depth_from_env(task_env);
    let families = if workflows_enabled {
        crate::control_socket::ToolFamilies::default()
    } else {
        crate::control_socket::ToolFamilies::fleet_only()
    };
    crate::control_socket::Grant::new(session, depth, max_depth)
        .with_families(families)
        .with_max_in_flight(max_in_flight)
        .with_tool_mode(tool_mode)
}

/// Mint this session's fleet grant against the control plane *this* process
/// serves, if it serves one.
///
/// `None` on every process that bound no control plane — a remote worker's
/// daemon, a `medulla workflow` invocation, a host with `mcp.fleetTools` off —
/// and that is exactly what leaves those sessions with no fleet verbs. Minted
/// per session rather than inherited: an inherited token would hand a second
/// harness the first one's capability.
pub fn local_fleet_grant(
    session: &str,
    task_env: &HashMap<String, String>,
    tool_mode: Option<&str>,
    workflows_enabled: bool,
) -> Option<(PathBuf, String)> {
    let plane = crate::control_socket::active()?;
    let grant = session_grant(
        session,
        task_env,
        tool_mode,
        workflows_enabled,
        plane.max_depth,
        plane.max_in_flight,
    );
    Some((plane.socket.clone(), plane.grants.mint(grant)))
}

/// Mint a hook-only credential for `session`'s built-in lifecycle hooks, if
/// this process serves a control plane for them to report to.
///
/// Unlike [`local_fleet_grant`], the result of this is meant to be written
/// straight into the harness's own environment — see
/// [`crate::control_socket::HOOK_SOCKET_ENV`] for why that is safe for this
/// grant and not for the fleet one. Minted under the *same* `session` key so
/// [`revoke_session`] gives back both this and any fleet grant with one call:
/// [`crate::control_socket::grants::GrantRegistry::revoke`] drops every grant
/// recorded under a session, not just one token's worth.
///
/// Never withheld for the reasons [`attach_cli`] withholds a fleet grant — an
/// overridden provider binary or a provider `supports_cli_attach` has not
/// verified — because a hook-only grant carries nothing an untrusted binary
/// could turn into a capability beyond filing reports under a session it
/// already runs as.
pub fn local_hook_grant(session: &str) -> Option<(PathBuf, String)> {
    let plane = crate::control_socket::active()?;
    let grant = crate::control_socket::Grant::hook_only(session);
    Some((plane.socket.clone(), plane.grants.mint(grant)))
}

/// Give back a session's grant once that session is over, and remove the
/// `--mcp-config` file it may have been minted into.
///
/// Best effort by design: a process with no control plane has nothing to
/// revoke against, a session that ended twice must not be an error, and a
/// session that never wrote a config file (no fleet grant, or the inline
/// document was enough) leaves nothing to remove. Called when a harness exits
/// so its token stops working, rather than living until the process does.
///
/// **A detached run outlives the harness that started it.** The default
/// `workflow_run` answers as soon as the run is going and leaves it executing
/// in the MCP subprocess, which stays alive for it after its parent's stdin
/// closes. So an agent that starts a workflow and then exits is the ordinary
/// case, not a rare one, and revoking outright here would cut the run's own
/// progress reports off mid-flight — the rail would freeze on whatever line
/// arrived last and never show how the run ended. When the registry says runs
/// are still executing, the grant is therefore *narrowed* to `run.report`
/// rather than dropped, and given back by the report that settles the last of
/// them (see
/// [`GrantRegistry::restrict_to_reporting`](crate::control_socket::grants::GrantRegistry::restrict_to_reporting)).
///
/// The run rows themselves are not dropped either way. They are drawn under a
/// PTY row whose screen is deliberately kept after its child exits, so they go
/// when that row goes — see [`forget_session_runs`].
pub fn revoke_session(session: &str) {
    if let Some(plane) = crate::control_socket::active() {
        if plane.runs.retire(session) {
            plane.grants.restrict_to_reporting(session);
        } else {
            plane.grants.revoke(session);
        }
    }
    // `None` for a session that failed `is_safe_session_component`: it never
    // reached a file (`write_config_file` refuses the same key), so there is
    // nothing here to remove — and, more to the point, nothing to guess a
    // path for that would not risk removing something unrelated.
    if let Some(path) = config_file_path(session) {
        let _ = std::fs::remove_file(path);
    }
}

/// Drop the workflow runs `session` reported, and any grant it still holds.
///
/// The counterpart to the retirement [`revoke_session`] performs: that keeps a
/// session's rows because the PTY row they are drawn under survives its child,
/// and this is called from the other end — when the operator drops that row —
/// so the table stays keyed by sessions something still draws.
///
/// Also revokes, which matters for the row forgotten while a run was still
/// executing: the reporting-only grant left alive for that run has nothing left
/// to report into, and leaving it would be a live token whose session is gone
/// from every view. Best effort, like everything else here.
pub fn forget_session_runs(session: &str) {
    if let Some(plane) = crate::control_socket::active() {
        plane.grants.revoke(session);
        plane.runs.forget(session);
    }
}

/// Attach Medulla's tools to a harness about to be started as a **CLI**.
///
/// Mutates the spawn the caller is assembling: the registration onto `args`,
/// nothing onto `env` — see the `env` note below. Returns the grant's session
/// key so the caller can [`revoke_session`] it when the harness exits, or
/// `None` when nothing was attached.
///
/// `session` is the key the grant is minted under and must be unique per
/// launch — two sessions sharing one key would share one capability, and
/// revoking either would revoke both.
///
/// Nothing is attached for a provider whose CLI has no registration flag we
/// have verified; see [`supports_cli_attach`]. That is a real gap rather than a
/// silent one: those providers still receive the tools over ACP, which is the
/// transport the orchestrator uses for delegated work.
///
/// `env` is read to resolve the session's policy (`workflows.enabled`, the
/// operator's tool mode) but never written: the secret half of a granted
/// session goes into a [`ServerSpec::write_config_file`] file, not this
/// process's or the harness's environment — see the module docs.
///
/// Withholds the fleet grant — but not workflow authoring, which carries no
/// secret — when `provider`'s binary has been redirected by one of its
/// [`crate::protocol::env`] overrides. Whatever that override names *is* the
/// process this registers `--mcp-config` onto, so it receives the file's path
/// on its own argv and can open it directly no matter how the secret reaches
/// it; per `AGENTS.md`, a provider-binary override is untrusted configuration,
/// so it is not handed a capability the real, verified CLI would only forward
/// to the one subprocess it names. When that withholding actually costs this
/// launch a grant it would otherwise have gotten — a control plane is
/// running, so `fleet_*` would exist for anyone else — `log` is told, once per
/// launch, so an operator running a legitimate wrapper binary sees *why*
/// `fleet_*` is missing rather than silently doing without it. `None` (every
/// caller with no log seam, and every test) means the note is simply not
/// sent; nothing about the decision itself changes.
pub fn attach_cli(
    provider: HarnessProvider,
    bin: &str,
    session: &str,
    env: &mut HashMap<String, String>,
    args: &mut Vec<String>,
    log: Option<&crate::logging::LineSink>,
) -> Option<String> {
    if !supports_cli_attach(provider) {
        return None;
    }
    // A launch serving a workflow `agent` node gets no Medulla tools at all —
    // see [`crate::harness_tools`] for why a step of a running graph must not
    // be able to start another one or dispatch into the pool its own run is
    // competing for. Checked before anything is minted so no grant is created
    // for a session that will not carry one.
    if crate::harness_tools::withheld(env) {
        return None;
    }
    let workflows_on = workflows_enabled(env);
    let tool_mode = env.get(super::TOOL_MODE_ENV).cloned();
    // The binary the caller is *actually about to launch*, not one re-derived
    // from `env`. A caller can hold two environments — the executor selects
    // its executable from `self.env` and hands the child a separately derived
    // one — and re-deriving here would clear a wrapper named only in the
    // other.
    let overridden = crate::protocol::env::bin_is_overridden(provider, bin);
    // Known ahead of minting so the log note below can say "withheld" only
    // when withholding actually cost this launch something: a host with no
    // control plane bound would have gotten no grant either way, and telling
    // an operator on such a host that their override "withheld" a fleet grant
    // would blame the wrong setting for a tool surface nothing here ever had.
    let would_serve_fleet = crate::control_socket::active().is_some();
    let fleet = if overridden || provider == HarnessProvider::Codex {
        None
    } else {
        local_fleet_grant(session, env, tool_mode.as_deref(), workflows_on)
    };
    let granted = fleet.is_some();
    if overridden && would_serve_fleet {
        if let Some(log) = log {
            // What is left depends on whether authoring survived: with
            // workflows off too, `server_spec` below returns `None` and this
            // launch gets no MCP server at all. Saying "workflow tools are
            // still attached" there would send an operator looking for a tool
            // surface that is entirely absent.
            let remainder = if workflows_on {
                "workflow tools are still attached"
            } else {
                "and workflows are off on this host, so no MCP tools are attached at all"
            };
            log(&format!(
                "mcp: {} is running as {bin} (overridden); the fleet grant was withheld — \
                 {remainder}",
                provider.as_str(),
            ));
        }
    }
    let spec = server_spec(tool_mode.as_deref(), fleet, workflows_on);
    let Some(spec) = spec else {
        // A grant was minted for a session that will not carry it. Given back
        // rather than left in the registry, where it would be a live token
        // nothing can ever redeem.
        if granted {
            revoke_session(session);
        }
        return None;
    };
    let registration = spec.endpoint(session);
    if provider == HarnessProvider::Codex {
        // Codex accepts the same `-c key=value` overrides in interactive and
        // `exec` modes. The launch marker is forwarded by name so its random
        // value does not land in `/proc/<pid>/cmdline`; fleet credentials stay
        // withheld because Codex has no verified server-only secret carrier.
        let attached = registration
            .env
            .iter()
            .find(|(key, _)| key == ATTACHED_ENV)
            .map(|(_, value)| value.clone())
            .expect("every server spec carries its attachment marker");
        env.insert(ATTACHED_ENV.to_string(), attached);
        args.extend(render_codex_config_args(&registration));
        return None;
    }

    args.push("--mcp-config".to_string());
    if spec.secret_env.is_empty() {
        // Nothing secret to keep off argv or out of the harness's own
        // environment, so the simplest registration — inline, no file to
        // clean up later — is also the safe one.
        args.push(claude_mcp_config(&registration));
        return granted.then(|| session.to_string());
    }
    match spec.write_config_file(session) {
        Ok(path) => {
            args.push(path.display().to_string());
            granted.then(|| session.to_string())
        }
        Err(_) => {
            // Could not write the file the grant would have gone in. The
            // inline document is still safe to fall back to — it never
            // renders `secret_env` — but a session with no way to reach its
            // grant has no use for one, so it is given back rather than left
            // a live token nothing can ever redeem.
            args.push(claude_mcp_config(&registration));
            if granted {
                revoke_session(session);
            }
            None
        }
    }
}

/// Whether a provider's CLI can be handed an MCP registration on its argv.
///
/// Claude Code takes a JSON document through `--mcp-config`; Codex takes
/// repeated `-c mcp_servers.medulla.*=...` overrides in both interactive and
/// exec modes. Both are verified per-launch channels. Other providers remain
/// unsupported until they have an equivalent invocation-scoped mechanism.
pub fn supports_cli_attach(provider: HarnessProvider) -> bool {
    medulla_harness::mcp::supports_cli_attach(provider)
}

/// Per-invocation Codex configuration for one explicitly attached server.

#[cfg(test)]
#[path = "attach_tests.rs"]
mod tests;
