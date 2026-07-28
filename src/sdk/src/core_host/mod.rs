//! Boot the embedded OpenHuman core in this process.
//!
//! One place builds the [`CoreRuntime`], so there is exactly one answer to
//! "which workspace does the core write to" and one place to change the domain
//! and service composition.
//!
//! # Workspace isolation is the load-bearing part
//!
//! OpenHuman resolves its own state directory from `OPENHUMAN_WORKSPACE`,
//! defaulting to `~/.openhuman/...`. Medulla resolves its state from
//! `MEDULLA_HOME`. Left alone those are independent, which quietly breaks the
//! scratch-run recipe this repo documents:
//!
//! ```text
//! MEDULLA_HOME=$(mktemp -d) ./target/debug/medulla
//! ```
//!
//! That recipe exists so a test run reads its own workflow store, agent
//! templates, and state rather than the developer's. Without deriving
//! `OPENHUMAN_WORKSPACE` from `MEDULLA_HOME`, every such run would still write
//! memory, flows, and credentials into the developer's real `~/.openhuman` —
//! silently, because nothing fails. [`workspace_dir`] is that derivation, and
//! it is why this module exists rather than callers building the runtime
//! themselves.
//!
//! # Composition
//!
//! [`DomainSet::embedded`] and [`ServiceSet::embedded`] describe a long-lived
//! host that drives the core in-process through the typed facade and owns its
//! own presentation layer — no HTTP listener, no Socket.IO, but the background
//! work a long session expects. Both are OpenHuman presets named for that
//! shape; see their docs for why this host is not `harness()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use openhuman_core::embed::Core;
use openhuman_core::{CoreBuilder, DomainSet, HostKind, ServiceSet, TokenSource};

#[cfg(test)]
mod tests;

/// Environment variable OpenHuman reads for its state directory.
pub const OPENHUMAN_WORKSPACE_ENV: &str = "OPENHUMAN_WORKSPACE";

/// Environment variable OpenHuman reads for the agent's read/write root.
pub const OPENHUMAN_ACTION_DIR_ENV: &str = "OPENHUMAN_ACTION_DIR";

/// The core's state directory for a given Medulla home.
///
/// Nested under the Medulla home rather than beside it so that removing a
/// scratch `MEDULLA_HOME` removes the core's state with it — a half-deleted
/// scratch run that leaves an OpenHuman workspace behind is worse than none,
/// because the next run silently inherits it.
pub fn workspace_dir(medulla_home: &Path) -> PathBuf {
    medulla_home.join("openhuman").join("workspace")
}

/// Point OpenHuman at a workspace derived from this process's Medulla home.
///
/// Idempotent and **non-overriding**: an operator who sets
/// `OPENHUMAN_WORKSPACE` explicitly keeps it, which is what lets a developer
/// aim the embedded core at an existing OpenHuman install on purpose. Returns
/// the directory in effect either way.
///
/// Call before [`boot`]; the core reads the variable during construction.
pub fn bind_workspace(env: &HashMap<String, String>, medulla_home: &Path) -> PathBuf {
    if let Some(explicit) = env
        .get(OPENHUMAN_WORKSPACE_ENV)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        let dir = PathBuf::from(explicit);
        tracing::debug!(
            "[core_host] workspace from {OPENHUMAN_WORKSPACE_ENV} (operator override): {}",
            dir.display()
        );
        return dir;
    }

    let dir = workspace_dir(medulla_home);
    std::env::set_var(OPENHUMAN_WORKSPACE_ENV, &dir);
    tracing::debug!(
        "[core_host] workspace derived from MEDULLA_HOME: {}",
        dir.display()
    );
    dir
}

/// Point the agent's read/write root at the operator's own workspace roots.
///
/// OpenHuman defaults `action_dir` to `~/OpenHuman/projects`, which is not
/// where a Medulla operator works — their repos are the workspace roots already
/// in Medulla's config. Leaving the default would aim the agent's write root at
/// a directory this host has never used.
///
/// Non-overriding for the same reason as [`bind_workspace`]. A `None` or empty
/// `root` leaves the variable alone rather than binding something arbitrary.
pub fn bind_action_dir(env: &HashMap<String, String>, root: Option<&Path>) -> Option<PathBuf> {
    if let Some(explicit) = env
        .get(OPENHUMAN_ACTION_DIR_ENV)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        return Some(PathBuf::from(explicit));
    }
    let root = root?;
    if root.as_os_str().is_empty() {
        return None;
    }
    std::env::set_var(OPENHUMAN_ACTION_DIR_ENV, root);
    tracing::debug!("[core_host] action_dir bound to {}", root.display());
    Some(root.to_path_buf())
}

/// Build the embedded core and wrap it in the typed facade.
///
/// Callers must have bound the workspace first — see [`bind_workspace`]. This
/// does not do it implicitly, because the binding reads process environment and
/// hiding that inside a constructor makes the ordering requirement invisible.
///
/// # Errors
///
/// Propagates any failure from [`CoreBuilder::build`] — a workspace that cannot
/// be created, or a token source that cannot be resolved.
pub async fn boot() -> anyhow::Result<Core> {
    tracing::debug!("[core_host] boot start host_kind=detect_standalone");
    let runtime = CoreBuilder::new(HostKind::detect_standalone())
        .domains(DomainSet::embedded())
        .services(ServiceSet::embedded())
        .token(TokenSource::EnvOrFile)
        .build()
        .await?;
    tracing::debug!("[core_host] boot ok services={:?}", runtime.services());
    Ok(Core::from_runtime(Arc::new(runtime)))
}
