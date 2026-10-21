use std::collections::HashMap;

use medulla_harness::harness_hooks::{launch_args, HookEvent, HookHandler, HookSpec, HooksConfig};
use medulla_types::HarnessProvider;

fn hooks() -> HooksConfig {
    HooksConfig {
        hooks: vec![HookSpec {
            event: HookEvent::SessionStart,
            matcher: "*".to_owned(),
            handler: HookHandler::Command {
                command: "echo hi".to_owned(),
                timeout: None,
            },
            harnesses: Vec::new(),
            label: None,
            builtin: false,
        }],
    }
}

#[test]
fn claude_hook_and_attribution_argv_matches_the_pre_extraction_snapshot() {
    let (argv, _) = launch_args(HarnessProvider::Claude, true, &hooks(), &HashMap::new());

    assert_eq!(
        argv,
        [
            "--settings",
            r#"{"attribution":{"commit":"Co-authored-by: Medulla <medulla@tinyhumans.ai>"},"hooks":{"SessionStart":[{"hooks":[{"command":"echo hi","type":"command"}],"matcher":"*"}]}}"#,
        ]
    );
}

#[test]
fn codex_hook_argv_matches_the_pre_extraction_snapshot() {
    let home = tempfile::tempdir().expect("temporary Codex home");
    let env = HashMap::from([(
        "CODEX_HOME".to_owned(),
        home.path().to_string_lossy().into_owned(),
    )]);
    let (argv, _) = launch_args(HarnessProvider::Codex, true, &hooks(), &env);

    assert_eq!(
        argv,
        [
            "-c",
            r#"hooks={"SessionStart"=[{"hooks"=[{"command"="echo hi","type"="command"}],"matcher"="*"}]}"#,
        ]
    );
}
