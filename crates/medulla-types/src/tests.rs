use super::*;

#[test]
fn primitive_types_keep_their_json_wire_values() {
    for (value, expected) in [
        (HarnessProvider::Claude, r#""claude""#),
        (HarnessProvider::Codex, r#""codex""#),
        (HarnessProvider::Opencode, r#""opencode""#),
        (HarnessProvider::Openhuman, r#""openhuman""#),
        (HarnessProvider::Shell, r#""shell""#),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), expected);
        assert_eq!(serde_json::from_str::<HarnessProvider>(expected).unwrap(), value);
    }
    for (value, expected) in [
        (HarnessTransport::Cli, r#""cli""#),
        (HarnessTransport::AppServer, r#""app_server""#),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), expected);
        assert_eq!(serde_json::from_str::<HarnessTransport>(expected).unwrap(), value);
    }
    for (value, expected) in [
        (BudgetWindow::Daily, r#""daily""#),
        (BudgetWindow::Weekly, r#""weekly""#),
        (BudgetWindow::FiveHour, r#""five_hour""#),
        (BudgetWindow::Unknown, r#""unknown""#),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), expected);
        assert_eq!(serde_json::from_str::<BudgetWindow>(expected).unwrap(), value);
    }
    let usage = TokenUsage { input_tokens: 17, output_tokens: 23 };
    let expected = r#"{"inputTokens":17,"outputTokens":23}"#;
    assert_eq!(serde_json::to_string(&usage).unwrap(), expected);
    assert_eq!(serde_json::from_str::<TokenUsage>(expected).unwrap(), usage);
}
