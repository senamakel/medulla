//! End-to-end coverage for the integrations doctor command.

use std::process::Command;

#[test]
fn doctor_json_has_one_secret_free_report_for_each_bundled_provider() {
    let output = Command::new(env!("CARGO_BIN_EXE_medulla"))
        .args(["integrations", "doctor", "--json"])
        .env_remove("MEDULLA_TOKEN")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .expect("doctor should start");

    assert!(output.status.success(), "{:?}", output);
    let reports: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor should emit JSON");
    let providers = reports
        .as_array()
        .expect("doctor output should be an array");
    assert_eq!(providers.len(), 3);
    for provider in providers {
        assert!(provider.get("id").is_some());
        assert!(provider.get("capabilities").is_some());
        assert!(provider.get("attentionRules").is_some());
    }
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(!rendered.contains("MEDULLA_TOKEN"));
    assert!(!rendered.contains("OPENAI_API_KEY"));
    assert!(!rendered.contains("ANTHROPIC_API_KEY"));
}
