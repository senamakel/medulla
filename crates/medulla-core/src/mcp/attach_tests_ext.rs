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
