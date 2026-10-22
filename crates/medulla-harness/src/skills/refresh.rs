//! Refreshing Medulla-managed skills from an abstract workflow catalog.

use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use medulla_types::HarnessProvider;

use super::install::sync;
use super::targets::{legacy_managed_root, managed_dir, managed_root, spawn_args};
use super::{
    CatalogSnapshot, InstallOptions, InstallReport, SkillScope, SkillTarget, WorkflowCatalog,
};

/// Refreshes one managed harness directory from a consistent catalog snapshot.
///
/// The catalog supplies the workflow listing, workspace identity, and Medulla
/// home, keeping filesystem policy independent of the workflow engine.
pub fn sync_catalog(
    catalog: &dyn WorkflowCatalog,
    target: SkillTarget,
) -> io::Result<InstallReport> {
    let snapshot = catalog.snapshot()?;
    sync_snapshot(&snapshot, target)
}

/// Refreshes skills and returns the provider arguments needed to expose them.
pub fn refresh_catalog(
    catalog: &dyn WorkflowCatalog,
    provider: HarnessProvider,
) -> io::Result<Vec<String>> {
    let Some(target) = managed_target(provider) else {
        return Ok(Vec::new());
    };
    let snapshot = catalog.snapshot()?;
    sync_snapshot(&snapshot, target)?;
    let dir = managed_dir(target, &snapshot.medulla_home, &snapshot.workspace_scope);
    Ok(spawn_args(provider, dir.is_dir().then_some(dir.as_path())))
}

fn sync_snapshot(snapshot: &CatalogSnapshot, target: SkillTarget) -> io::Result<InstallReport> {
    let root = managed_root(&snapshot.medulla_home, &snapshot.workspace_scope);
    let _lock = RefreshLock::acquire(&root)?;
    let options = InstallOptions {
        targets: vec![target],
        scope: SkillScope::Managed,
        root,
        with_commands: false,
        dry_run: false,
    };
    let report = sync(&snapshot.workflows, &options, snapshot.complete)?;
    if snapshot.complete {
        retire_unscoped(snapshot, target)?;
    }
    Ok(report)
}

fn retire_unscoped(snapshot: &CatalogSnapshot, target: SkillTarget) -> io::Result<()> {
    sync(
        &[],
        &InstallOptions {
            targets: vec![target],
            scope: SkillScope::Managed,
            root: legacy_managed_root(&snapshot.medulla_home),
            with_commands: false,
            dry_run: false,
        },
        true,
    )?;
    Ok(())
}

fn managed_target(provider: HarnessProvider) -> Option<SkillTarget> {
    match provider {
        HarnessProvider::Claude => Some(SkillTarget::Claude),
        _ => None,
    }
}

/// Exclusive lock for one workspace's managed skills tree.
pub struct RefreshLock {
    file: std::fs::File,
    path: PathBuf,
}

impl RefreshLock {
    /// Acquires the workspace lock, creating its parent directory if needed.
    pub fn acquire(root: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join(".refresh.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.lock_exclusive()?;
        Ok(Self { file, path })
    }
}

impl Drop for RefreshLock {
    fn drop(&mut self) {
        if let Err(source) = FileExt::unlock(&self.file) {
            tracing::warn!(path = %self.path.display(), "failed to release skill refresh lock: {source}");
        }
    }
}
