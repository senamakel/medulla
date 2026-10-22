use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use super::{refresh_catalog, sync_catalog, CatalogSnapshot, SkillInput, SkillTarget, SkillWorkflow};
use crate::skills::{managed_dir, skill_path, HarnessProvider, WorkflowCatalog};

struct FakeCatalog(Mutex<CatalogSnapshot>);

impl WorkflowCatalog for FakeCatalog {
    fn snapshot(&self) -> io::Result<CatalogSnapshot> {
        self.0
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| io::Error::other("catalog lock poisoned"))
    }
}

fn workflow(id: &str) -> SkillWorkflow {
    SkillWorkflow {
        id: id.to_string(),
        description: format!("Run {id}"),
        enabled: true,
        inputs: vec![SkillInput {
            name: "repo".into(),
            ty: "string".into(),
            description: None,
            required: true,
            default: None,
        }],
    }
}

fn catalog(home: PathBuf, scope: &str, workflows: Vec<SkillWorkflow>) -> FakeCatalog {
    FakeCatalog(Mutex::new(CatalogSnapshot {
        workflows,
        workspace_scope: scope.into(),
        medulla_home: home,
        complete: true,
    }))
}

#[test]
fn catalog_refresh_installs_and_returns_the_claude_managed_directory() {
    let home = tempfile::tempdir().unwrap();
    let catalog = catalog(home.path().to_path_buf(), "workspace-a", vec![workflow("audit")]);

    let args = refresh_catalog(&catalog, HarnessProvider::Claude).unwrap();
    let root = managed_dir(SkillTarget::Claude, home.path(), "workspace-a");

    assert_eq!(args, ["--add-dir", root.display().to_string()].map(str::to_owned));
    assert!(skill_path(SkillTarget::Claude, &root, "medulla-audit").is_file());
}

#[test]
fn catalog_refresh_prunes_only_in_its_workspace_scope() {
    let home = tempfile::tempdir().unwrap();
    let first = catalog(home.path().to_path_buf(), "workspace-a", vec![workflow("audit")]);
    let second = catalog(home.path().to_path_buf(), "workspace-b", vec![workflow("audit")]);
    sync_catalog(&first, SkillTarget::Claude).unwrap();
    sync_catalog(&second, SkillTarget::Claude).unwrap();

    first.0.lock().unwrap().workflows.clear();
    sync_catalog(&first, SkillTarget::Claude).unwrap();

    let first_root = managed_dir(SkillTarget::Claude, home.path(), "workspace-a");
    let second_root = managed_dir(SkillTarget::Claude, home.path(), "workspace-b");
    assert!(!skill_path(SkillTarget::Claude, &first_root, "medulla-audit").exists());
    assert!(skill_path(SkillTarget::Claude, &second_root, "medulla-audit").is_file());
}

#[test]
fn partial_catalog_does_not_prune_entries_missing_from_the_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let catalog = catalog(home.path().to_path_buf(), "workspace", vec![workflow("audit")]);
    sync_catalog(&catalog, SkillTarget::Claude).unwrap();
    {
        let mut snapshot = catalog.0.lock().unwrap();
        snapshot.workflows.clear();
        snapshot.complete = false;
    }

    sync_catalog(&catalog, SkillTarget::Claude).unwrap();
    let root = managed_dir(SkillTarget::Claude, home.path(), "workspace");
    assert!(skill_path(SkillTarget::Claude, &root, "medulla-audit").is_file());
}
