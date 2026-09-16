//! Unit tests for subcommand dispatch, the login/workflow flag parsers, help
//! text, and the `sessions` JSON.

use std::collections::HashMap;

use medulla::auth::Provider;
use medulla::protocol::HarnessProvider;

use super::argv;
use crate::cli::*;

#[test]
fn dispatches_subcommands() {
    assert_eq!(parse_command(&argv(&[])), Command::Tui);
    assert_eq!(parse_command(&argv(&["daemon", "--foo"])), Command::Daemon);
    assert_eq!(
        parse_command(&argv(&["daemon", "HK1-0000-AAAA", "--workspace", "/x"])),
        Command::DaemonHost,
        "a pasted host key selects the paired host"
    );
    assert_eq!(
        parse_command(&argv(&["daemon", "--host"])),
        Command::DaemonHost,
        "--host re-runs a pairing already on disk"
    );
    assert_eq!(
        parse_command(&argv(&["daemon", "--direct", "--peer-node", "00"])),
        Command::DaemonDirect,
        "the SSH-bootstrapped daemon is untouched"
    );
    assert_eq!(parse_command(&argv(&["version"])), Command::Version);
    assert_eq!(parse_command(&argv(&["-v"])), Command::Version);
    assert_eq!(parse_command(&argv(&["help"])), Command::Help);
    assert_eq!(parse_command(&argv(&["-h"])), Command::Help);
    assert_eq!(parse_command(&argv(&["sessions"])), Command::Sessions);
    assert_eq!(parse_command(&argv(&["login"])), Command::Login);
    assert_eq!(parse_command(&argv(&["logout"])), Command::Logout);
    assert_eq!(
        parse_command(&argv(&["codex", "resume"])),
        Command::Wrapper(HarnessProvider::Codex)
    );
    assert_eq!(
        parse_command(&argv(&["claude"])),
        Command::Wrapper(HarnessProvider::Claude)
    );
    assert_eq!(
        parse_command(&argv(&["opencode", "--foo"])),
        Command::Wrapper(HarnessProvider::Opencode)
    );
    assert_eq!(parse_command(&argv(&["update"])), Command::Update);
    assert_eq!(
        parse_command(&argv(&["update", "--check"])),
        Command::Update
    );
    assert_eq!(parse_command(&argv(&["--config", "x.json"])), Command::Tui);
}

#[test]
fn update_args_parse() {
    assert_eq!(parse_update_args(&argv(&[])), UpdateArgs { check: false });
    assert_eq!(
        parse_update_args(&argv(&["--check"])),
        UpdateArgs { check: true }
    );
    // Unknown flags are ignored.
    assert_eq!(
        parse_update_args(&argv(&["--check", "--force"])),
        UpdateArgs { check: true }
    );
}

#[test]
fn parses_tui_flags() {
    assert_eq!(parse_tui_args(&argv(&[])), TuiArgs::default());
    let a = parse_tui_args(&argv(&["--config", "c.json", "--no-alt-screen"]));
    assert_eq!(
        a,
        TuiArgs {
            config: Some("c.json".into()),
            alt_screen: false,
            mock: false,
        }
    );
    // A dangling --config keeps the default (None → layered discovery).
    assert_eq!(parse_tui_args(&argv(&["--config"])).config, None);
}

#[test]
fn help_names_the_binary() {
    let text = help_text();
    assert!(text.starts_with("medulla "));
    assert!(text.contains("--no-alt-screen"));
}

#[test]
fn login_args_parse() {
    assert_eq!(parse_login_args(&argv(&[])).unwrap(), LoginArgs::default());
    let a = parse_login_args(&argv(&[
        "--provider",
        "github",
        "--no-browser",
        "--token",
        "deadbeef",
        "--config",
        "c.json",
    ]))
    .unwrap();
    assert_eq!(a.provider, Provider::Github);
    assert!(a.no_browser);
    assert_eq!(a.token.as_deref(), Some("deadbeef"));
    assert_eq!(a.config.as_deref(), Some("c.json"));
    assert!(!a.code, "--code is off unless asked for");
    // Unknown provider is a friendly error.
    assert!(parse_login_args(&argv(&["--provider", "myspace"])).is_err());

    // The terminal flow is its own opt-in, and composes with --provider.
    let c = parse_login_args(&argv(&["--code", "--provider", "github"])).unwrap();
    assert!(c.code);
    assert_eq!(c.provider, Provider::Github);
    assert_eq!(c.token, None, "--code fetches its own code, none is given");
}

#[test]
fn help_text_carries_crate_version() {
    let text = help_text();
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    assert!(text.contains("medulla daemon"));
    assert!(text.contains("medulla login"));
    assert!(text.contains("medulla codex"));
    assert!(text.contains("--no-bridge"));
    assert!(text.contains("--provider"));
}

#[test]
fn sessions_json_is_valid_json_array() {
    // Point the scan dirs at an empty temp path so the result is deterministic
    // ([]), independent of the developer's real ~/.claude / ~/.codex history.
    let tmp = std::env::temp_dir().join(format!("medulla-cli-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let mut env = HashMap::new();
    env.insert(
        "MEDULLA_CLAUDE_SESSIONS_DIR".to_string(),
        tmp.join("claude").to_string_lossy().into_owned(),
    );
    env.insert(
        "MEDULLA_CODEX_SESSIONS_DIR".to_string(),
        tmp.join("codex").to_string_lossy().into_owned(),
    );
    let json = sessions_json(&env, tmp.to_str().unwrap()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn tui_args_default_and_debug() {
    let d = TuiArgs::default();
    assert_eq!(d.config, None);
    assert!(d.alt_screen);
    // Command derives Debug/Eq for assertions.
    assert!(format!("{:?}", Command::Tui).contains("Tui"));
    assert_ne!(Command::Tui, Command::Daemon);
}

#[test]
fn tui_args_parse_the_mock_flag() {
    // `--mock` is the only headless route to a working runtime with no backend
    // token: it must skip the login screen entirely.
    let a = parse_tui_args(&argv(&["--mock"]));
    assert!(a.mock);
    assert!(!parse_tui_args(&argv(&["--no-alt-screen"])).mock);
}

#[test]
fn help_text_documents_the_mock_flag() {
    assert!(help_text().contains("--mock"));
}

// The worker screen parses `--workspace` itself, because the daemon's flag
// types are private to the SDK. Pinned here because it decides which directory
// a *remote peer's* harness is allowed to edit.
#[test]
fn worker_flag_values_parse_in_both_spellings() {
    fn flag_value(args: &[String], name: &str) -> Option<String> {
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == name {
                return it.next().cloned().filter(|v| !v.is_empty());
            }
            if let Some(rest) = arg.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
                return (!rest.is_empty()).then(|| rest.to_string());
            }
        }
        None
    }
    let spaced: Vec<String> = ["--tui", "--workspace", "/repo"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(flag_value(&spaced, "--workspace").as_deref(), Some("/repo"));

    let equals: Vec<String> = ["--tui", "--workspace=/repo"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(flag_value(&equals, "--workspace").as_deref(), Some("/repo"));

    // Absent, or present with nothing after it, both mean "use the default" —
    // never an empty workspace, which would resolve to the filesystem root.
    let bare: Vec<String> = vec!["--tui".to_string()];
    assert_eq!(flag_value(&bare, "--workspace"), None);
    let dangling: Vec<String> = vec!["--workspace".to_string()];
    assert_eq!(flag_value(&dangling, "--workspace"), None);
}

#[test]
fn dispatches_the_workflow_subcommand_under_either_spelling() {
    assert_eq!(parse_command(&argv(&["workflow"])), Command::Workflow);
    assert_eq!(parse_command(&argv(&["workflows"])), Command::Workflow);
    assert_eq!(
        parse_command(&argv(&["workflow", "list"])),
        Command::Workflow
    );
}

#[test]
fn parses_every_workflow_verb_with_its_operand() {
    let cases: Vec<(Vec<&str>, WorkflowAction)> = vec![
        (vec![], WorkflowAction::List),
        (vec!["list"], WorkflowAction::List),
        (vec!["get", "sweep"], WorkflowAction::Get("sweep".into())),
        (
            vec!["create", "sweep"],
            WorkflowAction::Create("sweep".into()),
        ),
        (
            vec!["delete", "sweep"],
            WorkflowAction::Delete("sweep".into()),
        ),
        (
            vec!["apply-ops", "sweep"],
            WorkflowAction::ApplyOps("sweep".into()),
        ),
        (
            vec!["preview-ops", "sweep"],
            WorkflowAction::PreviewOps("sweep".into()),
        ),
        (
            vec!["dry-run", "sweep"],
            WorkflowAction::DryRun("sweep".into()),
        ),
        (vec!["run", "sweep"], WorkflowAction::Run("sweep".into())),
        (vec!["resume", "r1"], WorkflowAction::Resume("r1".into())),
        (vec!["cancel", "r1"], WorkflowAction::Cancel("r1".into())),
        (
            vec!["list-runs", "sweep"],
            WorkflowAction::ListRuns("sweep".into()),
        ),
        (vec!["get-run", "r1"], WorkflowAction::GetRun("r1".into())),
    ];

    for (args, expected) in cases {
        assert_eq!(
            parse_workflow_args(&argv(&args)).action,
            expected,
            "for {args:?}"
        );
    }
}

#[test]
fn workflow_defaults_reads_the_harness_and_model_flags() {
    let parsed = parse_workflow_args(&argv(&[
        "defaults",
        "sweep",
        "--harness",
        "codex",
        "--model",
        "gpt-5-codex",
    ]));

    assert_eq!(parsed.action, WorkflowAction::Defaults("sweep".into()));
    assert_eq!(parsed.harness.as_deref(), Some("codex"));
    assert_eq!(parsed.model.as_deref(), Some("gpt-5-codex"));
}

#[test]
fn workflow_defaults_with_no_flags_is_a_read() {
    // Absent is not the same as empty: absent leaves the setting alone (so the
    // verb prints it), empty clears it.
    let parsed = parse_workflow_args(&argv(&["defaults", "sweep"]));

    assert_eq!(parsed.action, WorkflowAction::Defaults("sweep".into()));
    assert_eq!(parsed.harness, None);
    assert_eq!(parsed.model, None);
}

#[test]
fn workflow_defaults_clears_with_an_empty_string() {
    let parsed = parse_workflow_args(&argv(&["defaults", "sweep", "--harness", ""]));

    assert_eq!(parsed.harness.as_deref(), Some(""));
}

#[test]
fn workflow_aliases_reach_the_same_actions() {
    assert_eq!(
        parse_workflow_args(&argv(&["show", "sweep"])).action,
        WorkflowAction::Get("sweep".into())
    );
    assert_eq!(
        parse_workflow_args(&argv(&["edit", "sweep"])).action,
        WorkflowAction::ApplyOps("sweep".into())
    );
    assert_eq!(
        parse_workflow_args(&argv(&["simulate", "sweep"])).action,
        WorkflowAction::DryRun("sweep".into())
    );
    assert_eq!(
        parse_workflow_args(&argv(&["kinds"])).action,
        WorkflowAction::Catalog(None)
    );
}

#[test]
fn validate_and_catalog_take_an_optional_operand() {
    // `validate` with no id reads a document from stdin; `catalog` with no kind
    // returns every contract. Both are useful bare, unlike the other verbs.
    assert_eq!(
        parse_workflow_args(&argv(&["validate"])).action,
        WorkflowAction::Validate(None)
    );
    assert_eq!(
        parse_workflow_args(&argv(&["validate", "sweep"])).action,
        WorkflowAction::Validate(Some("sweep".into()))
    );
    assert_eq!(
        parse_workflow_args(&argv(&["catalog", "agent"])).action,
        WorkflowAction::Catalog(Some("agent".into()))
    );
}

#[test]
fn a_verb_missing_its_required_operand_falls_back_to_listing() {
    // Listing is the harmless read-only answer to a half-typed command, and
    // matches how `workspace remove` already behaves.
    for verb in ["get", "run", "delete", "apply-ops", "resume", "cancel"] {
        assert_eq!(
            parse_workflow_args(&argv(&[verb])).action,
            WorkflowAction::List,
            "for bare {verb}"
        );
    }
}

#[test]
fn approve_and_reject_accumulate_so_one_resume_can_release_several_gates() {
    let parsed = parse_workflow_args(&argv(&[
        "resume",
        "r1",
        "--approve",
        "review",
        "--approve",
        "deploy",
        "--reject",
        "risky",
    ]));

    assert_eq!(parsed.action, WorkflowAction::Resume("r1".into()));
    assert_eq!(parsed.approve, vec!["review", "deploy"]);
    assert_eq!(parsed.reject, vec!["risky"]);
}

#[test]
fn workflow_flags_are_parsed_alongside_the_verb() {
    let parsed = parse_workflow_args(&argv(&[
        "run",
        "sweep",
        "--input",
        "{\"n\":1}",
        "--run-id",
        "r9",
        "--config",
        "/tmp/c.toml",
    ]));

    assert_eq!(parsed.action, WorkflowAction::Run("sweep".into()));
    assert_eq!(parsed.input.as_deref(), Some("{\"n\":1}"));
    assert_eq!(parsed.run_id.as_deref(), Some("r9"));
    assert_eq!(parsed.config.as_deref(), Some("/tmp/c.toml"));

    let missing = parse_workflow_args(&argv(&[
        "note",
        "sweep",
        "--kind",
        "--text",
        "--reason",
        "--supersedes",
        "--run-id",
        "r10",
    ]));
    assert_eq!(missing.kind, None);
    assert_eq!(missing.text, None);
    assert_eq!(missing.reason, None);
    assert!(missing.supersedes.is_empty());
    assert_eq!(missing.run_id.as_deref(), Some("r10"));
}

#[test]
fn help_text_documents_the_workflow_command() {
    let help = help_text();
    assert!(help.contains("medulla workflow"));
    assert!(help.contains("--approve"));
}

#[test]
fn the_mcp_verb_is_parsed_so_an_acp_session_can_launch_the_tool_server() {
    assert_eq!(
        parse_workflow_args(&argv(&["mcp"])).action,
        WorkflowAction::Mcp
    );
}

#[test]
fn mcp_is_a_top_level_command() {
    // What Medulla spawns for a harness. It must not fall through to the TUI:
    // an unrecognised subcommand starts the interactive app, and a harness that
    // got a ratatui screen on its stdio would hang rather than fail.
    assert_eq!(parse_command(&argv(&["mcp"])), Command::Mcp);
}

#[test]
fn help_text_documents_the_mcp_command() {
    let help = help_text();
    assert!(help.contains("medulla mcp"));
}
