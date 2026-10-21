//! Tests for the SDK-owned reserved tool-name vocabulary.

use super::is_reserved_tool_name;

#[test]
fn reserved_tool_names_match_the_ts_modules() {
    for name in [
        "task_create",
        "task_update",
        "task_list",
        "memory_write",
        "memory_read",
        "memory_list",
    ] {
        assert!(is_reserved_tool_name(name), "{name} should be reserved");
    }
    assert!(!is_reserved_tool_name("web_search"));
}
