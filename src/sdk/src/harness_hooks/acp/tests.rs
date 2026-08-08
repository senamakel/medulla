//! Unit tests for the ACP PostToolUse runner.

use serde_json::json;

use super::runner::{matches_tool, normalized_input, tool_matcher_candidates};

#[test]
fn matcher_preserves_codex_match_all_semantics() {
    assert!(matches_tool("*", "apply_patch"));
    assert!(matches_tool("^Bash$", "Bash"));
    assert!(!matches_tool("^Bash$", "apply_patch"));
}

/// A matcher written for Codex's native hook vocabulary still selects an ACP
/// call: the ACP `kind` is translated to the native tool names it corresponds
/// to, so `Bash` matches an `execute` and `Edit|Write` matches an `edit`.
#[test]
fn matcher_translates_acp_kinds_to_native_codex_tool_names() {
    assert!(matches_tool("Bash", "execute"));
    assert!(matches_tool("^Bash$", "execute"));
    assert!(matches_tool("Edit|Write", "edit"));
    assert!(matches_tool("^View$", "read"));
    assert!(!matches_tool("Bash", "edit"));
    assert!(!matches_tool("^Bash$", "read"));
}

#[test]
fn matcher_keeps_the_raw_kind_as_a_candidate() {
    assert!(matches_tool("execute", "execute"));
    assert!(tool_matcher_candidates("execute").contains(&"execute".to_string()));
    assert_eq!(tool_matcher_candidates("think"), vec!["think"]);
}

#[test]
fn normalizes_acp_path_for_codex_hook_consumers() {
    assert_eq!(
        normalized_input(&json!({"path":"a.rs"}))["file_path"],
        "a.rs"
    );
    assert_eq!(
        normalized_input(&json!({"filePath":"b.rs"}))["file_path"],
        "b.rs"
    );
    assert_eq!(
        normalized_input(&json!({"file_path":"c.rs"}))["file_path"],
        "c.rs"
    );
}
