//! Captured provider argv suffixes that were part of the SDK before extraction.

use medulla_providers::provider_args;
use medulla_types::HarnessProvider;
use std::collections::HashMap;

#[test]
fn provider_args_match_the_pre_extraction_argv_snapshots() {
    let cases = [
        (HarnessProvider::Claude, "MEDULLA_CLAUDE_ARGS", "--dangerously-skip-permissions --model opus", vec!["--dangerously-skip-permissions", "--model", "opus"]),
        (HarnessProvider::Codex, "MEDULLA_CODEX_ARGS", "--json --sandbox danger-full-access", vec!["--json", "--sandbox", "danger-full-access"]),
        (HarnessProvider::Opencode, "MEDULLA_OPENCODE_ARGS", "--format json --agent build", vec!["--format", "json", "--agent", "build"]),
        (HarnessProvider::Openhuman, "MEDULLA_OPENHUMAN_ARGS", "--profile local --verbose", vec!["--profile", "local", "--verbose"]),
        (HarnessProvider::Shell, "MEDULLA_SHELL_ARGS", "-lc 'printf hello'", vec!["-lc", "'printf", "hello'"]),
    ];

    for (provider, key, raw, expected) in cases {
        let env = HashMap::from([(key.to_string(), raw.to_string())]);
        assert_eq!(
            provider_args(provider, &env),
            expected.into_iter().map(str::to_string).collect::<Vec<_>>(),
            "provider {provider:?}"
        );
    }
}
