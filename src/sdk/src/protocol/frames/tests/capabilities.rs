//! Compatibility tests for additive worker capability fields.

use crate::protocol::{parse_agent_capabilities, AgentCapabilities, BudgetWindow};

#[test]
fn budget_window_defaults_to_unknown() {
    assert_eq!(BudgetWindow::default(), BudgetWindow::Unknown);
}

#[test]
fn screen_kill_support_is_additive_and_defaults_off_for_older_workers() {
    let older = parse_agent_capabilities(r#"{"providers":["claude"]}"#).unwrap();
    assert!(!older.screen_kill);

    let current = AgentCapabilities {
        screen_kill: true,
        ..Default::default()
    };
    let value = serde_json::to_value(&current).unwrap();
    assert_eq!(value["screenKill"], true);
}

#[test]
fn as_wire_is_the_exact_inverse_of_from_wire() {
    // The two names a provider has are easy to mix up: `as_str` is the display
    // name (`OpenCode`) and does not parse back. Asserting the round trip here
    // means a new provider cannot be added with only half the pair.
    for provider in [
        HarnessProvider::Claude,
        HarnessProvider::Codex,
        HarnessProvider::Opencode,
        HarnessProvider::Openhuman,
        HarnessProvider::Shell,
    ] {
        assert_eq!(
            HarnessProvider::from_wire(provider.as_wire()),
            Some(provider),
            "{provider:?} does not survive as_wire → from_wire"
        );
    }
}
