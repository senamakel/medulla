//! How the sidebar's grouping and sorting preferences change what the rail
//! lists — and, as much to the point, what they must not change: every agent
//! keeps a row and every session stays under its own agent whichever way the
//! two settings are turned.

use medulla::config::{SidebarGrouping, SidebarSort};
use medulla::runtime::AgentDeclaration;

use super::organize::sort_sessions;
use super::tests::{app, stub_session};
use super::{RailRow, SessionRailRow};
use crate::ui::app::App;

/// Three agents across two checkouts and two harnesses, in declaration order.
fn app_with_agents() -> App {
    let mut app = app();
    app.loaded.config.fleet.agent_declarations = vec![
        AgentDeclaration::new("zed", "", "claude", "/work/beta"),
        AgentDeclaration::new("acorn", "", "codex", "/work/alpha"),
        AgentDeclaration::new("mint", "", "claude", "/work/alpha"),
    ];
    app
}

/// The rail's sections: each header label (`None` for the unheaded top level)
/// with the agents listed under it, restricted to the declared agents so the
/// mock runtime's own folded lanes cannot make an assertion flaky.
fn sections(app: &App) -> Vec<(Option<String>, Vec<String>)> {
    let declared: Vec<String> = app
        .loaded
        .config
        .fleet
        .agent_declarations
        .iter()
        .map(|declaration| declaration.agent_id.clone())
        .collect();
    let mut sections: Vec<(Option<String>, Vec<String>)> = vec![(None, Vec::new())];
    for row in app.rail_rows() {
        match row {
            RailRow::Host(host) => sections.push((Some(host.label), Vec::new())),
            RailRow::Group(group) => sections.push((Some(group.label), Vec::new())),
            RailRow::Agent(agent) if declared.contains(&agent.agent_id) => {
                sections
                    .last_mut()
                    .expect("a section is always open")
                    .1
                    .push(agent.agent_id);
            }
            _ => {}
        }
    }
    // The mock runtime folds lanes of its own, which section themselves under
    // "no path"/"no harness"; those sections hold no declared agent and are
    // dropped so an assertion is about what the test declared.
    sections.retain(|(_, agents)| !agents.is_empty());
    sections
}

#[test]
fn the_default_leaves_one_machines_tree_unsectioned() {
    // The setting exists to be changed, not to change what an operator who has
    // never opened it sees: with one host and the default grouping the rail is
    // the flat list it was before grouping was configurable, in declaration
    // order.
    let app = app_with_agents();
    assert_eq!(
        app.loaded.config.appearance.sidebar_grouping,
        SidebarGrouping::Host
    );
    assert_eq!(
        sections(&app),
        vec![(None, vec!["zed".into(), "acorn".into(), "mint".into()])]
    );
}

#[test]
fn grouping_by_path_heads_each_checkout_once() {
    let mut app = app_with_agents();
    app.loaded.config.appearance.sidebar_grouping = SidebarGrouping::Path;

    assert_eq!(
        sections(&app),
        vec![
            (
                Some("/work/alpha".into()),
                vec!["acorn".into(), "mint".into()]
            ),
            (Some("/work/beta".into()), vec!["zed".into()]),
        ],
        "one header per directory, alphabetical, with its agents under it"
    );
}

#[test]
fn grouping_by_harness_sections_by_the_cli_each_agent_runs() {
    let mut app = app_with_agents();
    app.loaded.config.appearance.sidebar_grouping = SidebarGrouping::Harness;

    let sections = sections(&app);
    let claude = sections
        .iter()
        .find(|(label, _)| label.as_deref() == Some("claude"))
        .expect("the claude agents are sectioned together");
    assert_eq!(claude.1, vec!["zed".to_string(), "mint".to_string()]);
    assert!(sections
        .iter()
        .any(|(label, agents)| label.as_deref() == Some("codex") && agents == &["acorn"]));
}

#[test]
fn grouping_by_none_lists_every_agent_without_a_header() {
    let mut app = app_with_agents();
    app.loaded.config.appearance.sidebar_grouping = SidebarGrouping::None;

    assert_eq!(
        sections(&app),
        vec![(None, vec!["zed".into(), "acorn".into(), "mint".into()])]
    );
}

#[test]
fn no_grouping_loses_an_agent() {
    // The one property that has to hold across all four: sectioning is a
    // presentation of the same tree, so the set of agents on the rail is the
    // same set however it is grouped.
    let mut app = app_with_agents();
    let mut seen: Vec<Vec<String>> = Vec::new();
    for grouping in SidebarGrouping::ALL {
        app.loaded.config.appearance.sidebar_grouping = grouping;
        let mut agents: Vec<String> = sections(&app)
            .into_iter()
            .flat_map(|(_, agents)| agents)
            .collect();
        agents.sort();
        seen.push(agents);
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] == pair[1]),
        "every grouping lists the same agents: {seen:?}"
    );
}

#[test]
fn sorting_by_name_is_alphabetical_within_a_section() {
    let mut app = app_with_agents();
    app.loaded.config.appearance.sidebar_sort = SidebarSort::Name;

    assert_eq!(
        sections(&app),
        vec![(None, vec!["acorn".into(), "mint".into(), "zed".into()])]
    );
}

/// A session row for a live pty with the given start and last-output times.
fn session(id: &str, started_at: i64, last_output_at: i64) -> SessionRailRow {
    let mut local = stub_session(id);
    local.started_at = started_at;
    local.last_output_at = last_output_at;
    local.name = Some(id.to_string());
    SessionRailRow {
        agent_id: Some("agent".into()),
        lane_index: None,
        task: None,
        local: Some(local),
        last: false,
    }
}

/// The ids of `sessions` after sorting, for the session-order assertions.
fn sorted(mut sessions: Vec<SessionRailRow>, sort: SidebarSort) -> Vec<String> {
    sort_sessions(&mut sessions, sort);
    sessions
        .into_iter()
        .filter_map(|session| session.session_id().map(str::to_string))
        .collect()
}

#[test]
fn created_sorts_sessions_oldest_first() {
    let rows = vec![
        session("middle", 200, 500),
        session("oldest", 100, 900),
        session("newest", 300, 100),
    ];
    assert_eq!(
        sorted(rows, SidebarSort::Created),
        vec!["oldest", "middle", "newest"],
        "the default is the order a session list grows in"
    );
}

#[test]
fn recent_sorts_by_the_last_thing_a_session_did() {
    let rows = vec![
        session("quiet", 300, 100),
        session("loud", 100, 900),
        session("middling", 200, 500),
    ];
    assert_eq!(
        sorted(rows, SidebarSort::Recent),
        vec!["loud", "middling", "quiet"],
        "most recent output first, whatever order they started in"
    );
}

#[test]
fn name_sorts_sessions_by_what_the_operator_called_them() {
    let rows = vec![
        session("zulu", 100, 100),
        session("alpha", 200, 200),
        session("mike", 300, 300),
    ];
    assert_eq!(
        sorted(rows, SidebarSort::Name),
        vec!["alpha", "mike", "zulu"]
    );
}
