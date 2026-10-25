//! Session rows carry their agent process usage, and a list too large for one
//! message is split into chunks without losing a row.

use super::support::*;
use medulla::protocol::{AgentUsage, RemoteSessionRow, RemoteSessionState};

#[test]
fn session_usage_matches_classified_descendant_and_aggregated_tree() {
    use medulla::resources::agents::{
        discover_agents, AgentOrigin, KnownProcessRoot, ProcessRecord, ProcessUsage,
    };
    let process = |pid, ppid, argv: &[&str], cpu, rss| ProcessRecord {
        pid,
        ppid,
        exe: (pid == 11).then(|| std::path::PathBuf::from("/usr/bin/claude")),
        argv: Some(argv.iter().map(|arg| (*arg).to_owned()).collect()),
        cwd: None,
        started_at_secs: pid as u64,
        usage: ProcessUsage {
            cpu_percent: cpu,
            rss_bytes: rss,
            threads: 1,
            io: None,
        },
    };
    let processes = vec![
        process(10, None, &["pty-host"], 1.0, 100),
        process(11, Some(10), &["claude"], 4.0, 200),
        process(12, Some(11), &["git", "status"], 2.0, 50),
    ];
    let roots = vec![KnownProcessRoot {
        pid: 10,
        started_at_secs: 10,
        origin: AgentOrigin::Session { id: "w_10".into() },
    }];
    let agents = discover_agents(&processes, &roots, 99, 20);
    let agent = super::super::usage::classified_session_agent(&agents, "w_10")
        .expect("the discovered child agent belongs to the session root");
    assert_eq!(agent.pid, Some(11));
    assert_eq!(agent.kind, medulla::resources::agents::AgentKind::Claude);
    assert_eq!(agent.usage.cpu_percent, 6.0);
    assert_eq!(agent.usage.rss_bytes, 250);
}

#[test]
fn oversized_session_usage_is_trimmed_per_bounded_message() {
    let row = |id: &str, pid| RemoteSessionRow {
        id: id.into(),
        label: "agent".into(),
        provider: "claude".into(),
        preset: None,
        state: RemoteSessionState::Running,
        cwd: "/workspace".into(),
        name: None,
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_000_000,
        busy: true,
        working: true,
        attention: None,
        bracketed_paste: false,
        last_error: None,
        usage: Some(AgentUsage {
            kind: "Claude".into(),
            pid,
            cpu: 12.5,
            rss: 1024,
            threads: 5,
            started_at: 1_700_000_000,
            cpu_history: vec![12.5; 300],
        }),
    };
    let source = vec![row("w_1", 101), row("w_2", 202)];
    let messages = super::super::chunks::sessions_messages(&source, 6);
    let rows = messages
        .iter()
        .flat_map(|message| match message {
            RemoteMessage::Sessions { rows } | RemoteMessage::SessionsChunk { rows, .. } => rows,
            _ => panic!("expected a session list message"),
        })
        .collect::<Vec<_>>();
    assert!(messages.iter().all(|message| {
        encode_remote_message(message).len() <= medulla_link::MAX_MESSAGE_BYTES
    }));
    assert_eq!(rows.len(), 2, "usage trimming must preserve session rows");
    assert!(rows.iter().all(|row| row.usage.is_some()));
    assert!(rows.iter().all(|row| {
        row.usage
            .as_ref()
            .is_some_and(|usage| usage.cpu_history.len() < 300)
    }));
}

#[test]
fn oversized_base_session_lists_are_chunked_without_dropping_rows() {
    let rows = (0..80)
        .map(|index| RemoteSessionRow {
            id: format!("session-{index:03}"),
            label: format!("session label {index}"),
            provider: "shell".into(),
            preset: None,
            state: RemoteSessionState::Running,
            cwd: "/workspace/project".into(),
            name: None,
            thread_name: None,
            started_at: 1_700_000_000_000,
            last_output_at: 1_700_000_000_000,
            busy: false,
            working: false,
            attention: None,
            bracketed_paste: false,
            last_error: None,
            usage: None,
        })
        .collect::<Vec<_>>();
    let messages = super::super::chunks::sessions_messages(&rows, 7);
    assert!(messages.len() > 1);
    let mut received = Vec::new();
    for message in &messages {
        assert!(encode_remote_message(message).len() <= medulla_link::MAX_MESSAGE_BYTES);
        match message {
            RemoteMessage::SessionsChunk { rows, .. } => {
                received.extend(rows.iter().map(|row| row.id.clone()))
            }
            other => panic!("expected chunked list, got {other:?}"),
        }
    }
    assert_eq!(
        received,
        rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn oversized_usage_lists_keep_usage_on_chunks_that_fit_it() {
    let rows = (0..80)
        .map(|index| RemoteSessionRow {
            id: format!("agent-{index:03}"),
            label: format!("agent {index}"),
            provider: "claude".into(),
            preset: None,
            state: RemoteSessionState::Running,
            cwd: "/workspace/project".into(),
            name: None,
            thread_name: None,
            started_at: 1_700_000_000_000,
            last_output_at: 1_700_000_000_000,
            busy: true,
            working: true,
            attention: None,
            bracketed_paste: false,
            last_error: None,
            usage: Some(AgentUsage {
                kind: "Claude".into(),
                pid: 10_000 + index,
                cpu: 4.5,
                rss: 1024,
                threads: 5,
                started_at: 1_700_000_000,
                cpu_history: vec![4.5; 32],
            }),
        })
        .collect::<Vec<_>>();
    let messages = super::super::chunks::sessions_messages(&rows, 8);
    assert!(messages.len() > 1);
    let mut received = Vec::new();
    for message in &messages {
        assert!(encode_remote_message(message).len() <= medulla_link::MAX_MESSAGE_BYTES);
        let RemoteMessage::SessionsChunk { rows, .. } = message else {
            panic!("expected bounded chunks")
        };
        assert!(
            rows.iter().all(|row| row.usage.is_some()),
            "a chunk that fits its usage should retain it"
        );
        received.extend(rows.iter().map(|row| row.id.clone()));
    }
    assert_eq!(
        received,
        rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn oversized_multibyte_session_fields_are_safely_bounded() {
    let mut row = RemoteSessionRow {
        id: "unicode-row".into(),
        label: "界".repeat(2048),
        provider: "shell".into(),
        preset: Some("é".repeat(2048)),
        state: RemoteSessionState::Running,
        cwd: "/workspace".into(),
        name: None,
        thread_name: None,
        started_at: 1_700_000_000_000,
        last_output_at: 1_700_000_000_000,
        busy: false,
        working: false,
        attention: Some(medulla::protocol::RemoteAttention {
            summary: "é漢".repeat(2048),
            kind: "permission".into(),
        }),
        bracketed_paste: false,
        last_error: None,
        usage: None,
    };
    let messages = super::super::chunks::sessions_messages(&[row.clone()], 9);
    assert_eq!(messages.len(), 1);
    assert!(encode_remote_message(&messages[0]).len() <= medulla_link::MAX_MESSAGE_BYTES);
    let RemoteMessage::SessionsChunk { rows, .. } = &messages[0] else {
        panic!("oversized row should use a bounded chunk")
    };
    row = rows[0].clone();
    assert_eq!(row.id, "unicode-row");
    assert!(row.label.len() < "界".repeat(2048).len());
    assert!(row.preset.unwrap().len() < "é".repeat(2048).len());
    assert!(row.attention.unwrap().summary.len() < "é漢".repeat(2048).len());

    let mut boundary = "é漢".to_owned();
    super::super::chunks::truncate_to_char_boundary(&mut boundary, 3);
    assert_eq!(boundary, "é");
}
