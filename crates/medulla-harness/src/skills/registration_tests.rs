//! Unit tests for MCP server registration.
//!
//! The merge cases carry the weight: registering must never cost an operator
//! another MCP server or an unrelated setting, and re-running must not churn a
//! file that already says the right thing.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::registration::{register, RegistrationOptions};
use super::{SkillScope, SkillTarget};
use crate::workflows::mcp::ToolMode;

/// Registration options rooted at a scratch directory.
fn opts(root: &Path, targets: Vec<SkillTarget>, scope: SkillScope) -> RegistrationOptions {
    RegistrationOptions {
        targets,
        scope,
        root: root.to_path_buf(),
        project_dir: root.to_path_buf(),
        exe: PathBuf::from("/opt/medulla/bin/medulla"),
        tools_mode: "run".to_string(),
        dry_run: false,
    }
}

/// The parsed `.mcp.json` under `root`.
fn read_json(root: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(root.join(".mcp.json")).expect("read .mcp.json"))
        .expect("parse .mcp.json")
}

/// The parsed Codex config under `root`.
fn read_toml(root: &Path) -> toml::Table {
    fs::read_to_string(root.join(".codex").join("config.toml"))
        .expect("read config.toml")
        .parse()
        .expect("parse config.toml")
}

#[test]
fn claude_project_creates_mcp_json() {
    let dir = TempDir::new().unwrap();
    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Claude],
        SkillScope::Project,
    ))
    .unwrap();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].action, "created");
    assert_eq!(outcomes[0].path, Some(dir.path().join(".mcp.json")));
    assert!(outcomes[0].manual_command.is_none());
    assert_eq!(
        read_json(dir.path())["mcpServers"]["medulla"],
        json!({
            "command": "/opt/medulla/bin/medulla",
            "args": ["mcp"],
            "env": {"MEDULLA_WORKFLOW_TOOLS": "run"},
        })
    );
}


#[test]
fn claude_merge_preserves_other_servers_and_keys() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(".mcp.json"),
        serde_json::to_string_pretty(&json!({
            "$schema": "https://example.invalid/mcp.json",
            "mcpServers": {
                "linear": {"command": "linear-mcp", "args": ["serve"]},
            },
        }))
        .unwrap(),
    )
    .unwrap();

    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Claude],
        SkillScope::Project,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "updated");
    let document = read_json(dir.path());
    assert_eq!(
        document["$schema"],
        json!("https://example.invalid/mcp.json")
    );
    assert_eq!(
        document["mcpServers"]["linear"],
        json!({"command": "linear-mcp", "args": ["serve"]}),
        "the other server survives whole, not just its command"
    );
    assert_eq!(
        document["mcpServers"]["medulla"]["command"],
        json!("/opt/medulla/bin/medulla")
    );
}


#[test]
fn re_registering_is_unchanged_and_rewrites_nothing() {
    let dir = TempDir::new().unwrap();
    let options = opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    );
    register(&options).unwrap();
    let json_before = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    let toml_before = fs::read_to_string(dir.path().join(".codex").join("config.toml")).unwrap();

    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "unchanged");
    assert_eq!(again[1].action, "unchanged");
    assert_eq!(
        fs::read_to_string(dir.path().join(".mcp.json")).unwrap(),
        json_before
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".codex").join("config.toml")).unwrap(),
        toml_before
    );
}


#[test]
fn a_moved_binary_updates_the_entry() {
    let dir = TempDir::new().unwrap();
    let mut options = opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    );
    register(&options).unwrap();

    options.exe = PathBuf::from("/usr/local/bin/medulla");
    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "updated");
    assert_eq!(again[1].action, "updated");
    assert_eq!(
        read_json(dir.path())["mcpServers"]["medulla"]["command"],
        json!("/usr/local/bin/medulla")
    );
    assert_eq!(
        read_toml(dir.path())["mcp_servers"]["medulla"]["command"].as_str(),
        Some("/usr/local/bin/medulla")
    );
}


#[test]
fn a_changed_tools_mode_updates_the_entry() {
    let dir = TempDir::new().unwrap();
    let mut options = opts(dir.path(), vec![SkillTarget::Claude], SkillScope::Project);
    register(&options).unwrap();

    options.tools_mode = "full".to_string();
    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "updated");
    assert_eq!(
        read_json(dir.path())["mcpServers"]["medulla"]["env"]["MEDULLA_WORKFLOW_TOOLS"],
        json!("full")
    );
}


#[test]
fn an_unrecognised_tools_mode_is_refused_before_anything_is_written() {
    let dir = TempDir::new().unwrap();
    let mut options = opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    );
    options.tools_mode = "all".to_string();

    let error = register(&options).expect_err("`all` is not a ToolMode wire value");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    let message = error.to_string();
    assert!(message.contains('`'), "{message}");
    assert!(message.contains("all"), "{message}");
    assert!(message.contains("run"), "{message}");
    assert!(!dir.path().join(".mcp.json").exists());
    assert!(!dir.path().join(".codex").exists());
}


#[test]
fn a_tools_mode_that_withholds_workflow_run_is_refused() {
    let dir = TempDir::new().unwrap();
    let mut options = opts(dir.path(), vec![SkillTarget::Claude], SkillScope::Project);
    options.tools_mode = ToolMode::Propose.as_wire().to_string();

    let error = register(&options).expect_err("a propose session can never trigger a workflow");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("workflow_run"), "{error}");
    assert!(!dir.path().join(".mcp.json").exists());
}


#[test]
fn every_accepted_tools_mode_is_a_tool_mode_that_can_run() {
    let dir = TempDir::new().unwrap();
    for mode in [ToolMode::Full, ToolMode::Run] {
        let mut options = opts(dir.path(), vec![SkillTarget::Generic], SkillScope::Project);
        options.tools_mode = mode.as_wire().to_string();
        let outcomes = register(&options).expect("a runnable mode registers");
        assert!(
            outcomes[0]
                .manual_command
                .as_deref()
                .unwrap()
                .contains(&format!("MEDULLA_WORKFLOW_TOOLS={}", mode.as_wire())),
            "{outcomes:?}"
        );
    }
}


#[test]
fn dry_run_reports_the_same_outcomes_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let mut options = opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    );
    options.dry_run = true;

    let outcomes = register(&options).unwrap();

    assert_eq!(outcomes[0].action, "created");
    assert_eq!(outcomes[1].action, "created");
    assert!(!dir.path().join(".mcp.json").exists());
    assert!(!dir.path().join(".codex").exists());
}


#[test]
fn claude_user_scope_is_manual_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Claude],
        SkillScope::User,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "manual");
    assert_eq!(outcomes[0].path, None);
    assert_eq!(
        outcomes[0].manual_command.as_deref(),
        Some(
            "claude mcp add --scope user medulla --env MEDULLA_WORKFLOW_TOOLS=run -- /opt/medulla/bin/medulla mcp"
        )
    );
    assert!(!dir.path().join(".mcp.json").exists());
}


#[test]
fn generic_is_manual_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Generic],
        SkillScope::Project,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "manual");
    assert_eq!(outcomes[0].path, None);
    assert_eq!(
        outcomes[0].manual_command.as_deref(),
        Some(
            "configure your MCP client to run: MEDULLA_WORKFLOW_TOOLS=run /opt/medulla/bin/medulla mcp"
        )
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}


#[test]
fn an_unparseable_config_is_left_untouched() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".mcp.json"), "{ not json").unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    fs::write(dir.path().join(".codex").join("config.toml"), "= oops").unwrap();

    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "skipped");
    assert_eq!(outcomes[1].action, "skipped");
    assert!(outcomes[0].manual_command.is_some());
    assert!(outcomes[1].manual_command.is_some());
    assert_eq!(
        fs::read_to_string(dir.path().join(".mcp.json")).unwrap(),
        "{ not json"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".codex").join("config.toml")).unwrap(),
        "= oops",
        "a config we could not parse is left exactly as it was"
    );
}


#[test]
fn a_config_that_is_not_utf8_is_skipped_rather_than_failing_the_run() {
    // Registration happens *after* the skills are written, so returning an io
    // error here left the operator with installed skills, no report, and — under
    // --json — nothing on stdout to parse. A file we cannot decode is one we did
    // not write: same verdict as one we cannot parse.
    let dir = TempDir::new().unwrap();
    let claude = dir.path().join(".mcp.json");
    let codex = dir.path().join(".codex").join("config.toml");
    fs::write(&claude, [0xff, 0xfe, 0x00, 0x9f]).unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    fs::write(&codex, [0xe2, 0x28, 0xa1]).unwrap();

    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Claude, SkillTarget::Codex],
        SkillScope::Project,
    ))
    .expect("undecodable configs must not fail the run");

    for outcome in &outcomes {
        assert_eq!(outcome.action, "skipped", "{outcome:?}");
        let reason = outcome
            .manual_command
            .as_deref()
            .expect("a skip says why, and names the file");
        assert!(reason.contains("UTF-8"), "{reason}");
    }
    assert_eq!(fs::read(&claude).unwrap(), vec![0xff, 0xfe, 0x00, 0x9f]);
    assert_eq!(fs::read(&codex).unwrap(), vec![0xe2, 0x28, 0xa1]);
}
