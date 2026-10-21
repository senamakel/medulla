use std::collections::{HashMap, HashSet};

use super::types::{
    AgentKind, AgentOrigin, AgentProcess, IoCounters, KnownProcessRoot, ProcessNode, ProcessRecord,
    ProcessUsage,
};

/// Classify and aggregate recognized processes from a platform-neutral sample.
pub fn discover_agents(
    processes: &[ProcessRecord],
    known_roots: &[KnownProcessRoot],
    medulla_pid: u32,
    sampled_at_secs: u64,
) -> Vec<AgentProcess> {
    let by_pid: HashMap<u32, &ProcessRecord> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        if let Some(parent) = process.ppid {
            if by_pid.contains_key(&parent) {
                children.entry(parent).or_default().push(process.pid);
            }
        }
    }
    for pids in children.values_mut() {
        pids.sort_unstable();
    }

    let known: HashMap<u32, (&KnownProcessRoot, bool)> = known_roots
        .iter()
        .map(|root| {
            let current = by_pid
                .get(&root.pid)
                .is_some_and(|process| process.started_at_secs == root.started_at_secs);
            (root.pid, (root, current))
        })
        .collect();

    let mut candidates = Vec::new();
    for process in processes {
        let Some(kind) = classify(process) else {
            continue;
        };
        candidates.push((process, kind));
    }

    // npm can leave the package name in both its launcher argv and the Node
    // worker argv. Keep the deepest confirmed harness process as the row root.
    let candidate_pids: HashSet<u32> = candidates.iter().map(|(p, _)| p.pid).collect();
    candidates.retain(|(process, kind)| {
        !matches!(kind, AgentKind::ClaudeAcp | AgentKind::CodexAcp)
            || !has_descendant_candidate(process.pid, *kind, &children, &by_pid, &candidate_pids)
    });

    let mut agents = Vec::new();
    for (process, kind) in candidates {
        let Some(origin) = origin_for(process.pid, &known, medulla_pid, &by_pid) else {
            continue;
        };
        // A shell is a useful agent row only when Medulla can explain why it
        // exists. Reporting every user's login shell would swamp the pane.
        if kind == AgentKind::Shell && origin == AgentOrigin::External {
            continue;
        }
        let mut visiting = HashSet::new();
        visiting.insert(process.pid);
        let child_nodes = children
            .get(&process.pid)
            .into_iter()
            .flatten()
            .filter_map(|pid| make_node(*pid, &children, &by_pid, &mut visiting))
            .collect::<Vec<_>>();
        let mut usage = process.usage;
        let mut stack = child_nodes.iter().collect::<Vec<_>>();
        while let Some(node) = stack.pop() {
            add_usage(&mut usage, node.usage);
            stack.extend(node.children.iter());
        }
        agents.push(AgentProcess {
            pid: process.pid,
            kind,
            origin,
            exe: process.exe.clone(),
            argv: process.argv.clone(),
            cwd: process.cwd.clone(),
            started_at_secs: process.started_at_secs,
            runtime_secs: sampled_at_secs.saturating_sub(process.started_at_secs),
            usage,
            children: child_nodes,
        });
    }
    agents.sort_by_key(|agent| agent.pid);
    agents
}

fn classify(process: &ProcessRecord) -> Option<AgentKind> {
    let basename = process
        .exe
        .as_ref()
        .and_then(|exe| exe.file_name())
        .and_then(|name| name.to_str())?;
    let argv = process.argv.as_ref()?;
    let args = argv.iter().map(String::as_str).collect::<Vec<_>>();
    let has = |needle: &str| args.iter().any(|arg| arg.contains(needle));
    let has_arg = |needle: &str| args.iter().any(|arg| *arg == needle);

    match basename {
        "claude" => Some(AgentKind::Claude),
        "codex" if has_arg("app-server") => Some(AgentKind::CodexAppServer),
        "codex" => Some(AgentKind::Codex),
        "opencode" if has_arg("acp") => Some(AgentKind::OpencodeAcp),
        "opencode" => Some(AgentKind::Opencode),
        "openhuman-core" => Some(AgentKind::OpenhumanCore),
        "sh" | "bash" | "zsh" | "fish" | "pwsh" | "powershell" => Some(AgentKind::Shell),
        "node" | "npx" | "env"
            if has("@agentclientprotocol/claude-agent-acp") || has("claude-agent-acp") =>
        {
            Some(AgentKind::ClaudeAcp)
        }
        "node" | "npx" | "env" if has("@agentclientprotocol/codex-acp") || has("codex-acp") => {
            Some(AgentKind::CodexAcp)
        }
        _ => None,
    }
}

fn has_descendant_candidate(
    pid: u32,
    kind: AgentKind,
    children: &HashMap<u32, Vec<u32>>,
    by_pid: &HashMap<u32, &ProcessRecord>,
    candidate_pids: &HashSet<u32>,
) -> bool {
    let mut stack = children.get(&pid).cloned().unwrap_or_default();
    let mut visited = HashSet::new();
    while let Some(child) = stack.pop() {
        if !visited.insert(child) {
            continue;
        }
        if candidate_pids.contains(&child)
            && by_pid.get(&child).and_then(|process| classify(process)) == Some(kind)
        {
            return true;
        }
        stack.extend(children.get(&child).into_iter().flatten().copied());
    }
    false
}

fn origin_for(
    pid: u32,
    known: &HashMap<u32, (&KnownProcessRoot, bool)>,
    medulla_pid: u32,
    by_pid: &HashMap<u32, &ProcessRecord>,
) -> Option<AgentOrigin> {
    let mut current = Some(pid);
    let mut visited = HashSet::new();
    while let Some(at) = current {
        if !visited.insert(at) {
            return None;
        }
        if let Some((root, matches_start_time)) = known.get(&at) {
            if *matches_start_time {
                return Some(root.origin.clone());
            }
            // A reused root PID cannot confer its previous ownership.
            return Some(AgentOrigin::External);
        }
        if at == medulla_pid {
            return None;
        }
        current = by_pid.get(&at).and_then(|process| process.ppid);
    }
    Some(AgentOrigin::External)
}

fn make_node(
    pid: u32,
    children: &HashMap<u32, Vec<u32>>,
    by_pid: &HashMap<u32, &ProcessRecord>,
    visiting: &mut HashSet<u32>,
) -> Option<ProcessNode> {
    let process = *by_pid.get(&pid)?;
    if !visiting.insert(pid) {
        return None;
    }
    let nested = children
        .get(&pid)
        .into_iter()
        .flatten()
        .filter_map(|child| make_node(*child, children, by_pid, visiting))
        .collect();
    Some(ProcessNode {
        pid,
        exe: process.exe.clone(),
        usage: process.usage,
        children: nested,
    })
}

fn add_usage(total: &mut ProcessUsage, child: ProcessUsage) {
    total.cpu_percent += child.cpu_percent;
    total.rss_bytes = total.rss_bytes.saturating_add(child.rss_bytes);
    total.threads = total.threads.saturating_add(child.threads);
    match (&mut total.io, child.io) {
        (Some(total), Some(child)) => {
            total.read_bytes = total.read_bytes.saturating_add(child.read_bytes);
            total.written_bytes = total.written_bytes.saturating_add(child.written_bytes);
        }
        (None, Some(child)) => total.io = Some(child),
        _ => {}
    }
}
