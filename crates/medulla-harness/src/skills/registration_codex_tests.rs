//! Codex TOML registration preservation and idempotence checks.

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
fn codex_creates_config_toml() {
    let dir = TempDir::new().unwrap();
    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Codex],
        SkillScope::User,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "created");
    let config = read_toml(dir.path());
    let entry = config["mcp_servers"]["medulla"].as_table().unwrap();
    assert_eq!(entry["command"].as_str(), Some("/opt/medulla/bin/medulla"));
    assert_eq!(
        entry["args"],
        toml::Value::Array(vec![toml::Value::String("mcp".to_string())]),
        "args is exactly [\"mcp\"]"
    );
    assert_eq!(entry["env"]["MEDULLA_WORKFLOW_TOOLS"].as_str(), Some("run"));
}


#[test]
fn codex_merge_preserves_unrelated_tables_and_servers() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    fs::write(
        dir.path().join(".codex").join("config.toml"),
        "model = \"gpt-5\"\n\n[mcp_servers.linear]\ncommand = \"linear-mcp\"\nargs = [\"serve\"]\n",
    )
    .unwrap();

    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Codex],
        SkillScope::User,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "updated");
    let config = read_toml(dir.path());
    assert_eq!(config["model"].as_str(), Some("gpt-5"));
    assert_eq!(
        config["mcp_servers"]["linear"],
        "command = \"linear-mcp\"\nargs = [\"serve\"]\n"
            .parse::<toml::Table>()
            .map(toml::Value::Table)
            .unwrap(),
        "the other server survives whole, not just its command"
    );
    assert_eq!(
        config["mcp_servers"]["medulla"]["command"].as_str(),
        Some("/opt/medulla/bin/medulla")
    );
}

/// A hand-written `~/.codex/config.toml`: comments the operator wrote, a blank
/// line, and a key order that is not alphabetical.
const HAND_WRITTEN_CODEX_CONFIG: &str = "\
# Codex settings. Do not reformat.
model = \"gpt-5\"
approval_policy = \"on-request\"

# The linear server, added 2024-01-01.
[mcp_servers.linear]
command = \"linear-mcp\" # via npx
args = [\"serve\"]
";


#[test]
fn codex_registration_preserves_comments_blank_lines_and_key_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, HAND_WRITTEN_CODEX_CONFIG).unwrap();

    let outcomes = register(&opts(
        dir.path(),
        vec![SkillTarget::Codex],
        SkillScope::User,
    ))
    .unwrap();

    assert_eq!(outcomes[0].action, "updated");
    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.starts_with(HAND_WRITTEN_CODEX_CONFIG),
        "the operator's file must survive byte-for-byte, with our table appended:\n{after}"
    );
    let added = &after[HAND_WRITTEN_CODEX_CONFIG.len()..];
    assert!(
        added.contains("[mcp_servers.medulla]"),
        "our table is the only addition:\n{added}"
    );
    // And the added table means what it should.
    let config = read_toml(dir.path());
    assert_eq!(
        config["mcp_servers"]["medulla"]["command"].as_str(),
        Some("/opt/medulla/bin/medulla")
    );
    assert_eq!(
        config["mcp_servers"]["medulla"]["env"]["MEDULLA_WORKFLOW_TOOLS"].as_str(),
        Some("run")
    );
}


#[test]
fn codex_re_registration_over_a_hand_written_config_is_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, HAND_WRITTEN_CODEX_CONFIG).unwrap();
    let options = opts(dir.path(), vec![SkillTarget::Codex], SkillScope::User);
    register(&options).unwrap();
    let after_first = fs::read_to_string(&path).unwrap();

    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "unchanged");
    assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
}


#[test]
fn codex_keeps_the_operators_own_keys_inside_our_table() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "[mcp_servers.medulla]\n\
         command = \"/opt/medulla/bin/medulla\"\n\
         args = [\"mcp\"]\n\
         startup_timeout_sec = 30\n\
         env = { MEDULLA_WORKFLOW_TOOLS = \"run\", RUST_LOG = \"warn\" }\n",
    )
    .unwrap();
    let mut options = opts(dir.path(), vec![SkillTarget::Codex], SkillScope::User);
    let before = fs::read_to_string(&path).unwrap();

    let same = register(&options).unwrap();

    assert_eq!(
        same[0].action, "unchanged",
        "settings we do not write are not a difference"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), before);

    options.tools_mode = "full".to_string();
    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "updated");
    let entry = read_toml(dir.path())["mcp_servers"]["medulla"].clone();
    assert_eq!(
        entry["env"]["MEDULLA_WORKFLOW_TOOLS"].as_str(),
        Some("full")
    );
    assert_eq!(
        entry["env"]["RUST_LOG"].as_str(),
        Some("warn"),
        "the operator's own environment variable survives the mode change"
    );
    assert_eq!(entry["startup_timeout_sec"].as_integer(), Some(30));
}


#[test]
fn codex_update_keeps_the_operators_trivia_around_the_changed_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, HAND_WRITTEN_CODEX_CONFIG).unwrap();
    let mut options = opts(dir.path(), vec![SkillTarget::Codex], SkillScope::User);
    register(&options).unwrap();

    options.exe = PathBuf::from("/usr/local/bin/medulla");
    let again = register(&options).unwrap();

    assert_eq!(again[0].action, "updated");
    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.starts_with(HAND_WRITTEN_CODEX_CONFIG),
        "an update to our own table must not disturb the rest:\n{after}"
    );
    assert_eq!(
        read_toml(dir.path())["mcp_servers"]["medulla"]["command"].as_str(),
        Some("/usr/local/bin/medulla")
    );
}

