//! Tests for harness layouts, scope resolution, and spawn arguments.

use std::collections::HashMap;
use std::path::Path;

use tempfile::TempDir;

use super::*;
use medulla_types::HarnessProvider;

#[test]
fn every_target_has_its_documented_layout() {
    let root = Path::new("/root");

    assert_eq!(
        skill_path(SkillTarget::Claude, root, "medulla-x"),
        Path::new("/root/.claude/skills/medulla-x/SKILL.md")
    );
    assert_eq!(
        skill_path(SkillTarget::Codex, root, "medulla-x"),
        Path::new("/root/.agents/skills/medulla-x/SKILL.md")
    );
    assert_eq!(
        skill_path(SkillTarget::Generic, root, "medulla-x"),
        Path::new("/root/.agents/skills/medulla-x/SKILL.md")
    );
    assert_eq!(
        command_path(SkillTarget::Claude, root, "medulla-x").unwrap(),
        Path::new("/root/.claude/commands/medulla-x.md")
    );
    assert_eq!(
        command_path(SkillTarget::Codex, root, "medulla-x").unwrap(),
        Path::new("/root/.codex/prompts/medulla-x.md")
    );
    assert!(command_path(SkillTarget::Generic, root, "medulla-x").is_none());
}

#[test]
fn user_scope_uses_home_and_project_scope_uses_the_working_directory() {
    let env = HashMap::from([("HOME".to_string(), "/home/operator".to_string())]);
    let cwd = Path::new("/work/repo");
    assert_eq!(
        scope_root(SkillScope::User, &env, cwd),
        Path::new("/home/operator")
    );
    assert_eq!(scope_root(SkillScope::Project, &env, cwd), cwd);
    assert_eq!(scope_root(SkillScope::User, &HashMap::new(), cwd), cwd);
}

#[test]
fn managed_roots_are_scoped_by_catalog_home_and_workspace() {
    let home = Path::new("/medulla/home");
    let root = managed_root(home, "workspace-a");
    assert_eq!(root, home.join("skills/scopes/workspace-a"));
    assert_ne!(root, managed_root(home, "workspace-b"));
    assert_eq!(
        managed_dir(SkillTarget::Claude, home, "workspace-a"),
        root.join("claude-skills")
    );
    assert_eq!(
        managed_dir(SkillTarget::Codex, home, "workspace-a"),
        root.join("codex-skills")
    );
}

#[test]
fn claude_spawn_arguments_require_an_existing_managed_skills_root() {
    let home = TempDir::new().unwrap();
    let root = managed_dir(SkillTarget::Claude, home.path(), "workspace");
    assert!(spawn_args(HarnessProvider::Claude, Some(&root)).is_empty());
    assert!(spawn_args(HarnessProvider::Claude, None).is_empty());
    assert!(spawn_args(HarnessProvider::Codex, Some(&root)).is_empty());

    let path = skill_path(SkillTarget::Claude, &root, "medulla-audit");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "skill").unwrap();
    assert_eq!(
        spawn_args(HarnessProvider::Claude, Some(&root)),
        vec!["--add-dir".to_string(), root.display().to_string()]
    );
}

#[test]
fn default_targets_are_the_harnesses_already_present() {
    let home = TempDir::new().unwrap();
    assert!(default_targets(home.path()).is_empty());
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::create_dir_all(home.path().join(".agents")).unwrap();
    assert_eq!(
        default_targets(home.path()),
        vec![SkillTarget::Claude, SkillTarget::Generic]
    );
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    assert_eq!(
        default_targets(home.path()),
        vec![
            SkillTarget::Claude,
            SkillTarget::Codex,
            SkillTarget::Generic
        ]
    );
}

#[test]
fn target_and_scope_parse_round_trip() {
    for target in SkillTarget::ALL {
        assert_eq!(SkillTarget::parse(target.as_str()).unwrap(), target);
    }
    assert_eq!(SkillTarget::parse(" CLAUDE ").unwrap(), SkillTarget::Claude);
    assert!(SkillTarget::parse("cursor").is_err());
    assert_eq!(SkillScope::parse("User").unwrap(), SkillScope::User);
    assert_eq!(SkillScope::parse("project").unwrap(), SkillScope::Project);
    assert!(SkillScope::parse("global").is_err());
}
