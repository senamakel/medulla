//! Additional launch-policy tests for MCP attachment.

use super::*;

/// The shared policy both transports resolve through answers "no workflows"
/// for a launch whose workflow surface was withheld, whatever this host's
/// config says. That single answer is what makes the tools *absent*: it is
/// also what [`session_grant`] turns into a fleet-only grant.
#[test]
fn a_withheld_workflow_surface_is_disabled_for_that_launch() {
    let mut env = HashMap::new();
    assert!(
        workflows_enabled(&env),
        "an unmarked launch keeps the host's own answer"
    );

    crate::harness_tools::withhold_workflows(&mut env);

    assert!(!workflows_enabled(&env));
}

/// What the harness's MCP server is then served by: the fleet family alone.
#[test]
fn a_launch_without_workflows_is_granted_the_fleet_family_alone() {
    let env = HashMap::new();

    let grant = session_grant("acp-1", &env, None, false, 2, 4);

    assert_eq!(
        grant.families,
        crate::control_socket::ToolFamilies::fleet_only()
    );
}

/// With no fleet grant to serve either — a host running no control plane — a
/// withheld launch is registered no MCP server at all, rather than one that
/// would advertise nothing.
#[test]
fn a_withheld_workflow_surface_attaches_no_server_when_there_is_no_fleet() {
    let mut env = HashMap::new();
    crate::harness_tools::withhold_workflows(&mut env);
    let mut args = Vec::new();

    let attached = attach_cli(
        crate::protocol::HarnessProvider::Claude,
        "claude",
        "acp-2",
        &mut env,
        &mut args,
        None,
    );

    assert_eq!(attached, None);
    assert!(
        args.is_empty(),
        "no registration for a surface with nothing behind it: {args:?}"
    );
}

/// The end-to-end property `an_operator_started_claude_is_handed_medullas_own_tools`
/// (`crates/medulla-tui/src/ui/harness_pane/tests/session.rs`) cannot exercise on its own,
/// because that test never installs a control plane: with a real grant,
/// `attach_cli` must register through a file rather than the inline document,
/// and must never touch the process environment it was handed.
#[test]
fn attach_cli_never_writes_the_process_environment() {
    let mut env = HashMap::new();
    let mut args = Vec::new();
    attach_cli(
        crate::protocol::HarnessProvider::Claude,
        "claude",
        "attach-test-no-env-write",
        &mut env,
        &mut args,
        None,
    );
    assert!(
        env.is_empty(),
        "attach_cli must never write the harness's own environment: {env:?}"
    );
}

#[test]
fn attaching_a_cli_tells_the_server_which_session_it_serves() {
    // Without this the tool server cannot attribute a run it starts, and the
    // Agents rail has nothing to nest the run under.
    let mut env = HashMap::new();
    let mut args = Vec::new();
    attach_cli(
        crate::protocol::HarnessProvider::Claude,
        "claude",
        "pty-origin",
        &mut env,
        &mut args,
        None,
    );
    let document: Value = serde_json::from_str(&args[1]).expect("a JSON document");
    assert_eq!(
        document["mcpServers"]["medulla"]["env"][super::ORIGIN_SESSION_ENV],
        "pty-origin"
    );
    assert!(
        !env.contains_key(super::ORIGIN_SESSION_ENV),
        "it belongs to the server's own spawn, not to the harness's environment"
    );
}

#[test]
fn a_blank_session_key_stamps_nothing() {
    // A caller with no session to name must not write an empty attribution the
    // rail would then try to match runs against.
    let spec = ServerSpec {
        name: "medulla",
        command: "medulla".into(),
        args: vec!["mcp".into()],
        env: Vec::new(),
        secret_env: Vec::new(),
    }
    .for_session("   ");
    assert!(spec.env.is_empty());
}

/// The file transport carries the session too, so the ACP door attributes runs
/// exactly as the argv door does.
#[test]
fn a_written_config_file_carries_the_session_alongside_the_grant() {
    let _home = ScratchHome::install();
    let session = "attach-test-carries-session";
    let spec = server_spec(
        Some("run"),
        Some((PathBuf::from("/run/medulla.sock"), "s3cret".to_string())),
        true,
    )
    .expect("a server is served here")
    .for_session(session);

    let path = spec
        .write_config_file(session)
        .expect("the config file is writable");
    let contents = std::fs::read_to_string(&path).expect("the file was written");
    std::fs::remove_file(&path).ok();

    let document: Value = serde_json::from_str(&contents).expect("a JSON document");
    let env = &document["mcpServers"]["medulla"]["env"];
    assert_eq!(env[super::ORIGIN_SESSION_ENV], session);
    assert_eq!(
        env[crate::control_socket::MCP_GRANT_ENV],
        "s3cret",
        "and the grant is still there beside it"
    );
}
