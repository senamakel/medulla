//! Compatibility facade for the harness-owned workflow skill implementation.
//!
//! `medulla-harness` owns rendering and filesystem policy. This module adapts
//! Medulla's workflow engine types and keeps the existing SDK paths stable.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use medulla_harness::skills as harness_skills;
use medulla_types::HarnessProvider;

use super::{WorkflowInput, WorkflowSummary};

pub use harness_skills::{
    FileAction, FileOutcome, InstallOptions, InstallReport, InstalledSkill, RegistrationOptions,
    RegistrationOutcome, RenderedSkill, SkillInput, SkillScope, SkillTarget, SkillWorkflow,
    WorkflowCatalog,
};

/// Converts the engine's listing type to the harness-owned skill input shape.
pub fn skill_workflow(summary: &WorkflowSummary) -> SkillWorkflow {
    SkillWorkflow {
        id: summary.id.to_string(),
        description: summary.description.clone(),
        enabled: summary.enabled,
        inputs: summary.inputs.iter().map(skill_input).collect(),
    }
}

fn skill_input(input: &WorkflowInput) -> SkillInput {
    SkillInput {
        name: input.name.clone(),
        ty: input.ty.as_str().to_string(),
        description: input.description.clone(),
        required: input.required,
        default: input.default.clone(),
    }
}

/// Renders a workflow summary into its target-independent skill document.
pub fn render(summary: &WorkflowSummary) -> RenderedSkill {
    harness_skills::render(&skill_workflow(summary))
}

/// Renders a slash-command file using the same workflow inputs as its skill.
pub fn render_command(skill: &RenderedSkill, summary: &WorkflowSummary) -> String {
    harness_skills::render_command(skill, &skill_workflow(summary))
}

pub use harness_skills::slug_for;

/// Installs generated skill files for the selected harnesses.
pub fn install(
    workflows: &[WorkflowSummary],
    options: &InstallOptions,
) -> io::Result<InstallReport> {
    let workflows: Vec<_> = workflows.iter().map(skill_workflow).collect();
    harness_skills::install(&workflows, options)
}

/// Synchronizes generated skill files and optionally prunes removed workflows.
pub fn sync(
    workflows: &[WorkflowSummary],
    options: &InstallOptions,
    prune: bool,
) -> io::Result<InstallReport> {
    let workflows: Vec<_> = workflows.iter().map(skill_workflow).collect();
    harness_skills::sync(&workflows, options, prune)
}

/// Removes generated files for the named workflow ids.
pub fn uninstall(ids: &[String], options: &InstallOptions) -> io::Result<InstallReport> {
    harness_skills::uninstall(ids, options)
}

/// Lists managed skill files under the selected targets.
pub fn installed(options: &InstallOptions) -> io::Result<Vec<InstalledSkill>> {
    harness_skills::installed(options)
}

pub use harness_skills::register;
pub use harness_skills::{command_path, default_targets, skill_path};

/// Resolves a caller-facing scope using Medulla's home for managed skills.
pub fn scope_root(scope: SkillScope, env: &HashMap<String, String>, cwd: &Path) -> PathBuf {
    match scope {
        SkillScope::Managed => managed_root(env, cwd),
        _ => harness_skills::scope_root(scope, env, cwd),
    }
}

/// Medulla's workspace-specific root for harness-managed skills.
pub fn managed_root(env: &HashMap<String, String>, cwd: &Path) -> PathBuf {
    harness_skills::managed_root(
        &crate::home::medulla_home(env),
        &crate::workflows::store::workspace_scope(cwd),
    )
}

/// One harness's managed directory for the current Medulla home and workspace.
pub fn managed_dir(target: SkillTarget, env: &HashMap<String, String>, cwd: &Path) -> PathBuf {
    harness_skills::managed_dir(
        target,
        &crate::home::medulla_home(env),
        &crate::workflows::store::workspace_scope(cwd),
    )
}

/// Provider arguments that expose existing managed skills to a spawned harness.
pub fn spawn_args(
    provider: HarnessProvider,
    env: &HashMap<String, String>,
    cwd: &Path,
) -> Vec<String> {
    let dir = managed_dir(SkillTarget::Claude, env, cwd);
    harness_skills::spawn_args(provider, Some(&dir))
}

/// A workflow store backed by Medulla's discovery rules, adapted to the harness
/// catalog contract for refresh operations.
pub struct StoreCatalog {
    env: HashMap<String, String>,
    cwd: PathBuf,
}

impl StoreCatalog {
    /// Creates a catalog using the same environment and working directory that
    /// Medulla uses to discover workflow definitions.
    pub fn new(env: &HashMap<String, String>, cwd: &Path) -> Self {
        Self {
            env: env.clone(),
            cwd: cwd.to_path_buf(),
        }
    }
}

impl WorkflowCatalog for StoreCatalog {
    fn snapshot(&self) -> io::Result<harness_skills::CatalogSnapshot> {
        let report = crate::workflows::store::discover(&self.env, &self.cwd).load();
        Ok(harness_skills::CatalogSnapshot {
            workflows: report
                .workflows
                .iter()
                .map(|record| skill_workflow(&record.summary()))
                .collect(),
            workspace_scope: crate::workflows::store::workspace_scope(&self.cwd),
            medulla_home: crate::home::medulla_home(&self.env),
            complete: report.errors.is_empty(),
        })
    }
}

/// Refreshes skills before spawning a harness and returns its extra arguments.
pub fn refresh_managed(
    provider: HarnessProvider,
    env: &HashMap<String, String>,
    cwd: &Path,
) -> Vec<String> {
    if let Err(source) = harness_skills::refresh_catalog(&StoreCatalog::new(env, cwd), provider) {
        tracing::warn!(
            "could not refresh the managed skills; keeping the existing files: {source}"
        );
    }
    spawn_args(provider, env, cwd)
}

/// Refreshes one harness's managed skill directory from Medulla's workflow store.
pub fn sync_managed(
    target: SkillTarget,
    env: &HashMap<String, String>,
    cwd: &Path,
) -> io::Result<InstallReport> {
    harness_skills::sync_catalog(&StoreCatalog::new(env, cwd), target)
}
