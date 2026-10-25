//! The Agents table's rows: which processes are agents, and how the operator's
//! filter, sort, and tree settings arrange them.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::ui::app) struct AgentRow {
    pub host: Option<String>,
    pub pid: u32,
    pub started: u64,
    pub kind: String,
    pub origin: String,
    pub workspace: String,
    pub threads: usize,
    pub memory: u64,
    pub cpu: f64,
    pub runtime: u64,
    pub ppid: Option<u32>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub external: bool,
    pub remote: bool,
    pub depth: usize,
}

impl AgentRow {
    pub(in crate::ui::app) fn identity(&self) -> AgentIdentity {
        AgentIdentity {
            host: self.host.clone(),
            pid: self.pid,
            started: self.started,
        }
    }
}

pub(in crate::ui::app) fn rows(snapshot: &ResourceSnapshot) -> Vec<AgentRow> {
    let registrations = medulla::resources::launch::agent_registry().snapshot();
    snapshot
        .processes
        .iter()
        .filter_map(|process| {
            let registered = registrations.iter().find(|entry| {
                entry.pid == process.pid && entry.started_at_secs == process.start_time
            });
            let detected = detect_kind(process);
            let kind = registered
                .map(|entry| pretty_kind(&entry.provider))
                .or(detected)?;
            let (origin, session_id, task_id, external) = match registered {
                Some(entry) if entry.session_id.is_some() => (
                    entry.session_id.clone().unwrap_or_default(),
                    entry.session_id.clone(),
                    entry.task_id.clone(),
                    false,
                ),
                Some(entry) if entry.task_id.is_some() => (
                    format!("task {}", entry.task_id.as_deref().unwrap_or_default()),
                    None,
                    entry.task_id.clone(),
                    false,
                ),
                Some(_) => ("host".into(), None, None, false),
                None => ("external".into(), None, None, true),
            };
            let remote = session_id.as_deref().is_some_and(|id| {
                matches!(
                    crate::ui::app::remote_sessions::SessionRef::parse(id),
                    crate::ui::app::remote_sessions::SessionRef::Remote { .. }
                )
            });
            Some(AgentRow {
                host: None,
                pid: process.pid,
                started: process.start_time,
                kind,
                origin,
                workspace: process.cwd.clone().unwrap_or_else(|| "—".into()),
                threads: process.threads,
                memory: process.memory_bytes,
                cpu: process.cpu_percent,
                runtime: (medulla::clock::now_millis()
                    - (process.start_time.saturating_mul(1000) as i64))
                    .max(0) as u64
                    / 1000,
                ppid: process.ppid,
                session_id,
                task_id,
                external,
                remote,
                depth: 0,
            })
        })
        .collect()
}

pub(super) fn detect_kind(process: &ProcessSnapshot) -> Option<String> {
    let full = format!(
        "{} {}",
        process.exe.as_deref().unwrap_or_default(),
        process.cmd.join(" ")
    )
    .to_lowercase();
    if full.contains("codex-acp") {
        Some("Codex-ACP".into())
    } else if full.contains("claude") {
        Some("Claude".into())
    } else if full.contains("openhuman") || full.contains("medulla") {
        Some("OpenHuman".into())
    } else if full.contains("aider")
        || full.contains("gemini")
        || full.contains("goose")
        || full.contains("cursor")
        || full.contains("codex")
        || full.contains("opencode")
    {
        Some("External".into())
    } else {
        None
    }
}

pub(super) fn pretty_kind(provider: &str) -> String {
    match HarnessProvider::from_wire(provider) {
        Some(HarnessProvider::Claude) => "Claude".into(),
        Some(HarnessProvider::Codex) => "Codex".into(),
        Some(HarnessProvider::Opencode) => "OpenCode".into(),
        Some(HarnessProvider::Openhuman) => "OpenHuman".into(),
        Some(_) => provider.to_owned(),
        None if provider.to_lowercase().contains("openhuman") => "OpenHuman".into(),
        None if provider.to_lowercase().contains("codex-acp") => "Codex-ACP".into(),
        None if provider.to_lowercase().contains("claude") => "Claude".into(),
        None => provider.to_owned(),
    }
}

pub(in crate::ui::app) fn visible_rows(
    snapshot: &ResourceSnapshot,
    state: &ResourcesState,
) -> Vec<AgentRow> {
    let mut rows = rows(snapshot);
    if let Some(filter) = state.agent_filter.as_deref() {
        let (regex, pattern) = filter
            .strip_prefix('!')
            .map_or((false, filter), |pattern| (true, pattern));
        if regex {
            if let Ok(pattern) = regex::Regex::new(&format!("(?i){pattern}")) {
                rows.retain(|row| pattern.is_match(&row.kind));
            } else {
                rows.clear();
            }
        } else {
            let pattern = pattern.to_lowercase();
            rows.retain(|row| {
                format!("{} {} {} {}", row.pid, row.kind, row.origin, row.workspace)
                    .to_lowercase()
                    .contains(&pattern)
            });
        }
    }
    rows.sort_by(|a, b| compare(a, b, state.agent_sort));
    if state.agent_reverse {
        rows.reverse();
    }
    if state.agent_tree {
        rows = tree_rows(snapshot, rows);
    }
    rows
}

pub(in crate::ui::app) fn visible_rows_with_remote(
    snapshot: &ResourceSnapshot,
    remote_hosts: &[(String, Vec<medulla::protocol::RemoteSessionRow>)],
    state: &ResourcesState,
) -> Vec<AgentRow> {
    let mut rows = visible_rows(snapshot, state);
    let mut hosts: Vec<_> = remote_hosts.iter().collect();
    hosts.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (host, sessions) in hosts {
        let mut host_rows = Vec::new();
        for session in sessions {
            let Some(usage) = &session.usage else {
                continue;
            };
            let row = AgentRow {
                host: Some(host.clone()),
                pid: usage.pid,
                started: usage.started_at,
                kind: usage.kind.clone(),
                origin: format!("Remote{{{host}}}"),
                workspace: session.cwd.clone(),
                threads: usage.threads,
                memory: usage.rss,
                cpu: usage.cpu,
                runtime: (medulla::clock::now_millis()
                    - usage.started_at.saturating_mul(1000) as i64)
                    .max(0) as u64
                    / 1000,
                ppid: None,
                session_id: Some(crate::ui::app::remote_sessions::qualify(host, &session.id)),
                task_id: None,
                external: false,
                remote: true,
                depth: 0,
            };
            if matches_filter(&row, state.agent_filter.as_deref()) {
                host_rows.push(row);
            }
        }
        host_rows.sort_by(|a, b| compare(a, b, state.agent_sort));
        if state.agent_reverse {
            host_rows.reverse();
        }
        rows.extend(host_rows);
    }
    rows
}

pub(super) fn matches_filter(row: &AgentRow, filter: Option<&str>) -> bool {
    let Some(filter) = filter else { return true };
    let (regex, pattern) = filter
        .strip_prefix('!')
        .map_or((false, filter), |pattern| (true, pattern));
    if regex {
        regex::Regex::new(&format!("(?i){pattern}"))
            .is_ok_and(|pattern| pattern.is_match(&row.kind))
    } else {
        format!("{} {} {} {}", row.pid, row.kind, row.origin, row.workspace)
            .to_lowercase()
            .contains(&pattern.to_lowercase())
    }
}

pub(super) fn tree_rows(snapshot: &ResourceSnapshot, agents: Vec<AgentRow>) -> Vec<AgentRow> {
    let all_agents: std::collections::HashMap<u32, AgentRow> =
        agents.iter().cloned().map(|row| (row.pid, row)).collect();
    let mut children: std::collections::HashMap<u32, Vec<&ProcessSnapshot>> =
        std::collections::HashMap::new();
    for process in &snapshot.processes {
        if let Some(parent) = process.ppid {
            children.entry(parent).or_default().push(process);
        }
    }
    for descendants in children.values_mut() {
        descendants.sort_by_key(|process| process.pid);
    }
    let roots: Vec<AgentRow> = agents
        .iter()
        .filter(|row| {
            row.ppid
                .is_none_or(|parent| !all_agents.contains_key(&parent))
        })
        .cloned()
        .collect();
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    for root in roots {
        let pid = root.pid;
        visited.insert(pid);
        result.push(root);
        append_children(pid, 1, &children, &all_agents, &mut visited, &mut result);
    }
    // Cycles or incomplete parent chains should not make a known agent vanish.
    for row in agents {
        if visited.insert(row.pid) {
            result.push(row);
        }
    }
    result
}

pub(super) fn append_children(
    parent: u32,
    depth: usize,
    children: &std::collections::HashMap<u32, Vec<&ProcessSnapshot>>,
    agents: &std::collections::HashMap<u32, AgentRow>,
    visited: &mut std::collections::HashSet<u32>,
    result: &mut Vec<AgentRow>,
) {
    if let Some(descendants) = children.get(&parent) {
        for process in descendants {
            if !visited.insert(process.pid) {
                continue;
            }
            let mut row = agents
                .get(&process.pid)
                .cloned()
                .unwrap_or_else(|| AgentRow {
                    host: None,
                    pid: process.pid,
                    started: process.start_time,
                    kind: "Process".into(),
                    origin: "child process".into(),
                    workspace: process.cwd.clone().unwrap_or_else(|| "—".into()),
                    threads: process.threads,
                    memory: process.memory_bytes,
                    cpu: process.cpu_percent,
                    runtime: (medulla::clock::now_millis()
                        - process.start_time.saturating_mul(1000) as i64)
                        .max(0) as u64
                        / 1000,
                    ppid: process.ppid,
                    session_id: None,
                    task_id: None,
                    external: true,
                    remote: false,
                    depth,
                });
            row.depth = depth;
            let pid = row.pid;
            result.push(row);
            append_children(pid, depth + 1, children, agents, visited, result);
        }
    }
}

pub(super) fn compare(a: &AgentRow, b: &AgentRow, key: AgentSort) -> Ordering {
    match key {
        AgentSort::Pid => a.pid.cmp(&b.pid),
        AgentSort::Kind => a.kind.cmp(&b.kind),
        AgentSort::Origin => a.origin.cmp(&b.origin),
        AgentSort::Workspace => a.workspace.cmp(&b.workspace),
        AgentSort::Threads => a.threads.cmp(&b.threads),
        AgentSort::Memory => a.memory.cmp(&b.memory),
        AgentSort::Cpu => a.cpu.total_cmp(&b.cpu),
        AgentSort::Runtime => a.runtime.cmp(&b.runtime),
    }
}

pub(super) fn kind_counts(rows: &[AgentRow]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.kind.as_str()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|(kind, count)| format!("{kind} {count}"))
        .collect::<Vec<_>>()
        .join(" · ")
}
