//! Unit tests for agent classification and discovery.

use std::path::PathBuf;

use super::{
    discover_agents, discover_agents_with_options, discover_registered_agents,
    AgentDiscoveryOptions, AgentKind, AgentOrigin, IoCounters, KnownProcessRoot, ProcessRecord,
    ProcessUsage,
};

fn process(
    pid: u32,
    ppid: Option<u32>,
    exe: &str,
    argv: &[&str],
    started_at_secs: u64,
) -> ProcessRecord {
    ProcessRecord {
        pid,
        ppid,
        exe: Some(PathBuf::from(exe)),
        argv: Some(argv.iter().map(|arg| (*arg).to_string()).collect()),
        cwd: Some(PathBuf::from("/workspace/project")),
        started_at_secs,
        usage: ProcessUsage {
            cpu_percent: 1.0,
            rss_bytes: 100,
            threads: 2,
            io: Some(IoCounters {
                read_bytes: 10,
                written_bytes: 20,
            }),
        },
    }
}

#[test]
fn claude_under_pty_is_session_owned_with_nested_children_and_aggregates() {
    let processes = vec![
        process(10, Some(1), "/usr/bin/pty-host", &["pty-host"], 5),
        process(100, Some(10), "/usr/bin/claude", &["claude"], 20),
        process(101, Some(100), "/usr/bin/git", &["git", "status"], 21),
    ];
    let roots = [KnownProcessRoot {
        pid: 10,
        started_at_secs: 5,
        origin: AgentOrigin::Session { id: "w_1".into() },
    }];

    let agents = discover_agents(&processes, &roots, 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].kind, AgentKind::Claude);
    assert_eq!(agents[0].origin, AgentOrigin::Session { id: "w_1".into() });
    assert_eq!(agents[0].usage.cpu_percent, 2.0);
    assert_eq!(agents[0].usage.rss_bytes, 200);
    assert_eq!(agents[0].usage.threads, 4);
    assert_eq!(agents[0].usage.io.unwrap().read_bytes, 20);
    assert_eq!(agents[0].runtime_secs, 30);
    assert_eq!(agents[0].children[0].pid, 101);
}

#[test]
fn env_npx_node_acp_chain_is_one_claude_acp_agent() {
    let processes = vec![
        process(
            20,
            Some(999),
            "/usr/bin/env",
            &[
                "env",
                "NODE_OPTIONS=--x",
                "npx",
                "-y",
                "@agentclientprotocol/claude-agent-acp@latest",
            ],
            1,
        ),
        process(
            21,
            Some(20),
            "/usr/bin/npx",
            &["npx", "-y", "@agentclientprotocol/claude-agent-acp@latest"],
            2,
        ),
        process(
            22,
            Some(21),
            "/usr/bin/node",
            &["node", "/home/user/.npm/claude-agent-acp/dist/index.js"],
            3,
        ),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].pid, Some(22));
    assert_eq!(agents[0].kind, AgentKind::ClaudeAcp);
    assert_eq!(agents[0].origin, AgentOrigin::External);
}

#[test]
fn external_claude_is_classified_and_unrelated_node_is_ignored() {
    let processes = vec![
        process(30, Some(7), "/opt/bin/claude", &["claude"], 10),
        process(31, Some(7), "/usr/bin/node", &["node", "server.js"], 11),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].pid, Some(30));
    assert_eq!(agents[0].kind, AgentKind::Claude);
    assert_eq!(agents[0].origin, AgentOrigin::External);
}

#[test]
fn root_pid_reuse_does_not_attribute_agent_to_stale_session() {
    let processes = vec![
        process(10, Some(7), "/usr/bin/pty-host", &["pty-host"], 20),
        process(40, Some(10), "/usr/bin/claude", &["claude"], 30),
    ];
    let roots = [KnownProcessRoot {
        pid: 10,
        started_at_secs: 10,
        origin: AgentOrigin::Session {
            id: "w_stale".into(),
        },
    }];

    let agents = discover_agents(&processes, &roots, 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].origin, AgentOrigin::External);
}

#[test]
fn registered_launch_roots_are_used_by_resource_discovery() {
    let _guard = crate::resources::launch::agent_registry().register_at(
        crate::resources::launch::AgentRegistration {
            pid: 10,
            started_at_secs: 5,
            task_id: Some("task-10".into()),
            session_id: Some("session-10".into()),
            provider: "claude".into(),
        },
    );
    let processes = vec![
        process(10, Some(1), "/usr/bin/pty-host", &["pty-host"], 5),
        process(100, Some(10), "/usr/bin/claude", &["claude"], 20),
    ];

    let agents = discover_registered_agents(&processes, 1, 50, AgentDiscoveryOptions::default());

    assert_eq!(agents.len(), 1);
    assert_eq!(
        agents[0].origin,
        AgentOrigin::Task {
            id: "task-10".into()
        }
    );
}

#[test]
fn medulla_and_unowned_tool_descendants_are_not_external() {
    let processes = vec![
        process(1, None, "/usr/bin/medulla", &["medulla"], 1),
        process(2, Some(1), "/usr/bin/git", &["git", "status"], 2),
        process(3, Some(1), "/usr/bin/claude", &["claude"], 3),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].kind, AgentKind::EmbeddedOpenhuman);
    assert_eq!(agents[0].children.len(), 1);
    assert_eq!(agents[0].children[0].pid, 2);
}

#[test]
fn embedded_openhuman_share_contains_medullas_direct_shell_tool_children() {
    let processes = vec![
        process(1, None, "/usr/bin/medulla", &["medulla"], 1),
        process(2, Some(1), "/usr/bin/bash", &["bash", "-lc", "echo hi"], 2),
        process(3, Some(2), "/usr/bin/git", &["git", "status"], 3),
        process(
            4,
            Some(1),
            "/usr/bin/curl",
            &["curl", "https://example.test"],
            4,
        ),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);

    assert_eq!(agents.len(), 1);
    let embedded = &agents[0];
    assert_eq!(embedded.kind, AgentKind::EmbeddedOpenhuman);
    assert_eq!(embedded.pid, None);
    assert_eq!(embedded.children.len(), 2);
    assert_eq!(embedded.children[0].pid, 2);
    assert_eq!(embedded.children[0].children[0].pid, 3);
    assert_eq!(embedded.children[1].pid, 4);
}

#[test]
fn wrappers_are_not_classified_when_their_deepest_harness_is_visible() {
    let processes = vec![
        process(
            50,
            Some(999),
            "/usr/bin/npx",
            &["npx", "@agentclientprotocol/codex-acp@latest"],
            1,
        ),
        process(51, Some(50), "/usr/bin/node", &["node", "codex-acp"], 2),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].pid, Some(51));
    assert_eq!(agents[0].kind, AgentKind::CodexAcp);
}

#[test]
fn app_server_and_opencode_acp_have_distinct_kinds() {
    let processes = vec![
        process(60, Some(999), "/usr/bin/codex", &["codex", "app-server"], 1),
        process(
            62,
            Some(999),
            "C:/Program Files/Codex/Codex.Exe",
            &["Codex", "app-server"],
            1,
        ),
        process(61, Some(999), "/usr/bin/opencode", &["opencode", "acp"], 1),
    ];

    let agents = discover_agents(&processes, &[], 1, 50);

    assert_eq!(agents.len(), 3);
    assert_eq!(
        agents
            .iter()
            .filter(|agent| agent.kind == AgentKind::CodexAppServer)
            .count(),
        2
    );
    assert!(agents
        .iter()
        .any(|agent| agent.kind == AgentKind::OpencodeAcp));
}

#[test]
fn unreadable_argv_is_unknown_and_external_instead_of_guessing() {
    let mut process = process(70, Some(7), "/opt/bin/claude", &["claude"], 10);
    process.argv = None;

    let agents = discover_agents(&[process], &[], 1, 50);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].kind, AgentKind::Unknown);
    assert_eq!(agents[0].origin, AgentOrigin::External);
}

#[test]
fn external_shells_are_included_only_when_requested() {
    let processes = vec![process(80, Some(7), "/usr/bin/bash", &["bash", "-l"], 10)];

    assert!(discover_agents(&processes, &[], 1, 50).is_empty());
    let agents = discover_agents_with_options(
        &processes,
        &[],
        1,
        50,
        AgentDiscoveryOptions {
            show_external_shells: true,
        },
    );

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].kind, AgentKind::Shell);
    assert_eq!(agents[0].origin, AgentOrigin::External);
}

/// Claude's native build runs from a versioned file name; the name it was
/// launched as still identifies it.
#[test]
fn a_harness_on_a_versioned_executable_is_named_by_its_argv() {
    let processes = vec![process(
        100,
        Some(1),
        "/Users/me/.local/share/claude/versions/2.1.310",
        &["claude", "--session-id", "abc"],
        5,
    )];
    let agents = discover_agents(&processes, &[], 999, 50);
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].kind, AgentKind::Claude);
}

/// Codex's app-server daemon supervises an app-server child: one harness, one
/// row, with the child's usage folded into it.
#[test]
fn a_harness_that_re_execs_itself_is_one_row() {
    let codex = "/Users/me/.codex/bin/codex";
    let processes = vec![
        process(200, Some(1), codex, &["codex", "app-server", "daemon"], 5),
        process(
            201,
            Some(200),
            codex,
            &["codex", "app-server", "--listen"],
            6,
        ),
        process(
            202,
            Some(201),
            "/Users/me/.codex/bin/codex-code-mode-host",
            &["codex-code-mode-host"],
            7,
        ),
    ];
    let agents = discover_agents(&processes, &[], 999, 50);
    assert_eq!(agents.len(), 1, "{agents:?}");
    assert_eq!(agents[0].pid, Some(200));
    assert_eq!(agents[0].kind, AgentKind::CodexAppServer);
    assert_eq!(agents[0].usage.rss_bytes, 300);
}

/// A harness started through an interpreter is named by the script it runs.
#[test]
fn interpreted_harnesses_are_named_by_their_script() {
    let processes = vec![
        process(
            300,
            Some(1),
            "/usr/bin/node",
            &["node", "/usr/lib/node_modules/.bin/gemini"],
            5,
        ),
        process(
            301,
            Some(1),
            "/usr/bin/python3",
            &["python3", "/Users/me/.local/bin/aider", "--yes"],
            5,
        ),
        process(
            302,
            Some(1),
            "/usr/bin/node",
            &["node", "/srv/app/server.js"],
            5,
        ),
    ];
    let kinds: Vec<_> = discover_agents(&processes, &[], 999, 50)
        .into_iter()
        .map(|agent| agent.kind)
        .collect();
    assert_eq!(kinds, [AgentKind::Gemini, AgentKind::Aider]);
}

/// An editor whose install path mentions an agent, an MCP server, and another
/// Medulla are not harnesses.
#[test]
fn programs_that_only_mention_a_harness_are_not_agents() {
    let processes = vec![
        process(
            400,
            Some(1),
            "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Plugin).app/Contents/MacOS/Cursor Helper (Plugin)",
            &["Cursor Helper (Plugin): extension-host"],
            5,
        ),
        process(401, Some(1), "/usr/local/bin/chrome-devtools-mcp", &["chrome-devtools-mcp", "--claude"], 5),
        process(402, Some(1), "/Users/me/.medulla/bin/medulla", &["medulla", "mcp"], 5),
        process(403, Some(1), "/Users/me/.codex/bin/codex-code-mode-host", &["codex-code-mode-host"], 5),
    ];
    assert!(discover_agents(&processes, &[], 999, 50).is_empty());
}

/// An npm-installed Claude Code runs as `node …/@anthropic-ai/claude-code/cli.js`;
/// only that package entry point counts, not any script called `cli.js`.
#[test]
fn npm_installed_claude_code_is_recognized_by_its_package_entry_point() {
    let processes = vec![
        process(
            400,
            Some(1),
            "/usr/bin/node",
            &[
                "node",
                "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            ],
            5,
        ),
        process(
            401,
            Some(1),
            "/usr/bin/node",
            &["node", "/srv/app/cli.js", "--claude"],
            5,
        ),
    ];
    let agents = discover_agents(&processes, &[], 999, 50);
    assert_eq!(agents.len(), 1, "{agents:?}");
    assert_eq!(agents[0].pid, Some(400));
    assert_eq!(agents[0].kind, AgentKind::Claude);
}
