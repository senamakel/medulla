use std::collections::{HashMap, HashSet};

use super::types::{
    AgentDiscoveryOptions, AgentKind, AgentOrigin, AgentProcess, KnownProcessRoot, ProcessNode,
    ProcessRecord, ProcessUsage,
};

/// Classify and aggregate recognized processes from a platform-neutral sample.
pub fn discover_agents(
    processes: &[ProcessRecord],
    known_roots: &[KnownProcessRoot],
    medulla_pid: u32,
    sampled_at_secs: u64,
) -> Vec<AgentProcess> {
    discover_agents_with_options(
        processes,
        known_roots,
        medulla_pid,
        sampled_at_secs,
        AgentDiscoveryOptions::default(),
    )
}

/// Discover agents with explicit controls for optional process categories.
pub fn discover_agents_with_options(
    processes: &[ProcessRecord],
    known_roots: &[KnownProcessRoot],
    medulla_pid: u32,
    sampled_at_secs: u64,
    options: AgentDiscoveryOptions,
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
    // Anything else that re-execs itself — Codex's app-server daemon and the
    // server it supervises — is one harness: keep the outermost process as the
    // row and let the rest aggregate beneath it.
    let kinds: HashMap<u32, AgentKind> = candidates.iter().map(|(p, k)| (p.pid, *k)).collect();
    candidates.retain(|(process, kind)| {
        matches!(
            kind,
            AgentKind::ClaudeAcp | AgentKind::CodexAcp | AgentKind::Shell
        ) || !has_ancestor_of_kind(process, *kind, &kinds, &by_pid)
    });

    let mut agents = Vec::new();
    for (process, kind) in candidates {
        let Some(origin) = origin_for(process.pid, &known, medulla_pid, &by_pid) else {
            continue;
        };
        // A shell is a useful agent row only when Medulla can explain why it
        // exists. Reporting every user's login shell would swamp the pane.
        if kind == AgentKind::Shell
            && origin == AgentOrigin::External
            && !options.show_external_shells
        {
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
            pid: Some(process.pid),
            kind,
            origin,
            exe: process.exe.clone(),
            cwd: process.cwd.clone(),
            started_at_secs: process.started_at_secs,
            runtime_secs: sampled_at_secs.saturating_sub(process.started_at_secs),
            usage,
            children: child_nodes,
        });
    }
    if let Some(embedded) =
        embedded_openhuman(medulla_pid, sampled_at_secs, &children, &by_pid, &known)
    {
        agents.push(embedded);
    }
    agents.sort_by_key(|agent| agent.pid.unwrap_or(0));
    agents
}

fn embedded_openhuman(
    medulla_pid: u32,
    sampled_at_secs: u64,
    children: &HashMap<u32, Vec<u32>>,
    by_pid: &HashMap<u32, &ProcessRecord>,
    known: &HashMap<u32, (&KnownProcessRoot, bool)>,
) -> Option<AgentProcess> {
    let root = *by_pid.get(&medulla_pid)?;
    let direct_children = children
        .get(&medulla_pid)?
        .iter()
        .copied()
        .filter(|pid| {
            if known.get(pid).is_some_and(|(_, current)| *current) {
                return false;
            }
            by_pid
                .get(pid)
                .and_then(|process| classify(process))
                .is_none_or(|kind| kind == AgentKind::Shell)
        })
        .collect::<Vec<_>>();
    if direct_children.is_empty() {
        return None;
    }

    let mut visiting = HashSet::from([medulla_pid]);
    let nested = direct_children
        .into_iter()
        .filter_map(|pid| make_node(pid, children, by_pid, &mut visiting))
        .collect::<Vec<_>>();
    let mut usage = root.usage;
    let mut stack = nested.iter().collect::<Vec<_>>();
    while let Some(node) = stack.pop() {
        add_usage(&mut usage, node.usage);
        stack.extend(node.children.iter());
    }
    Some(AgentProcess {
        pid: None,
        kind: AgentKind::EmbeddedOpenhuman,
        origin: AgentOrigin::Headless,
        exe: None,
        cwd: root.cwd.clone(),
        started_at_secs: root.started_at_secs,
        runtime_secs: sampled_at_secs.saturating_sub(root.started_at_secs),
        usage,
        children: nested,
    })
}

/// Interpreters whose first non-flag argument names the program they run, so
/// `node /usr/lib/node_modules/.bin/gemini` is Gemini and not just Node.
const INTERPRETERS: [&str; 8] = [
    "node", "bun", "deno", "python", "python3", "env", "npx", "uv",
];

const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "fish", "pwsh", "powershell"];

/// Lowercased file name of `path` without a Windows `.exe` suffix.
fn program_name(path: &str) -> Option<String> {
    let name = std::path::Path::new(path).file_name()?.to_str()?;
    let name = name.to_ascii_lowercase();
    Some(name.strip_suffix(".exe").map(str::to_owned).unwrap_or(name))
}

/// Map one program name to the harness it is, if any.
fn harness_named(name: &str, has_arg: &dyn Fn(&str) -> bool) -> Option<AgentKind> {
    match name {
        "claude" => Some(AgentKind::Claude),
        "claude-agent-acp" | "claude-code-acp" => Some(AgentKind::ClaudeAcp),
        "codex" if has_arg("app-server") => Some(AgentKind::CodexAppServer),
        "codex" => Some(AgentKind::Codex),
        "codex-acp" => Some(AgentKind::CodexAcp),
        "opencode" if has_arg("acp") => Some(AgentKind::OpencodeAcp),
        "opencode" => Some(AgentKind::Opencode),
        "openhuman-core" | "openhuman" => Some(AgentKind::OpenhumanCore),
        "gemini" => Some(AgentKind::Gemini),
        "aider" => Some(AgentKind::Aider),
        "goose" => Some(AgentKind::Goose),
        "cursor-agent" => Some(AgentKind::CursorAgent),
        _ => None,
    }
}

/// Whether a process whose short name is `name` could be a harness, a shell,
/// or an interpreter running one — the cheap pre-filter a sampler applies
/// before paying to read a process's executable and argv.
pub fn may_be_agent_name(name: &str) -> bool {
    let Some(name) = program_name(name) else {
        return false;
    };
    SHELLS.contains(&name.as_str())
        || name == "dash"
        || INTERPRETERS.contains(&name.as_str())
        || harness_named(&name, &|_| false).is_some()
}

/// Classify one process on its own, without its tree.
pub fn classify_process(process: &ProcessRecord) -> Option<AgentKind> {
    classify(process)
}

fn classify(process: &ProcessRecord) -> Option<AgentKind> {
    let exe_name = process
        .exe
        .as_ref()
        .and_then(|exe| exe.to_str())
        .and_then(program_name);
    let argv = match &process.argv {
        Some(argv) => argv,
        None => {
            let name = exe_name?;
            let known =
                SHELLS.contains(&name.as_str()) || harness_named(&name, &|_| false).is_some();
            return known.then_some(AgentKind::Unknown);
        }
    };
    let args = argv.iter().map(String::as_str).collect::<Vec<_>>();
    let has = |needle: &str| args.iter().any(|arg| arg.contains(needle));
    let has_arg = |needle: &str| args.contains(&needle);

    // A harness can be installed under a versioned file name (Claude's native
    // build runs as `…/claude/versions/2.1.310`), so the name it was launched
    // as counts as much as the file it was launched from.
    let argv0_name = args.first().and_then(|arg| program_name(arg));
    let names = || exe_name.iter().chain(argv0_name.iter());
    if let Some(kind) = names().find_map(|name| harness_named(name, &has_arg)) {
        return Some(kind);
    }
    if names().any(|name| SHELLS.contains(&name.as_str())) {
        return Some(AgentKind::Shell);
    }
    if !names().any(|name| INTERPRETERS.contains(&name.as_str())) {
        return None;
    }
    if has("@agentclientprotocol/claude-agent-acp") || has("claude-agent-acp") {
        return Some(AgentKind::ClaudeAcp);
    }
    if has("@agentclientprotocol/codex-acp") || has("codex-acp") {
        return Some(AgentKind::CodexAcp);
    }
    if has("@google/gemini-cli") {
        return Some(AgentKind::Gemini);
    }
    let script = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .and_then(|arg| program_name(arg))?;
    harness_named(&script, &has_arg)
}

fn has_ancestor_of_kind(
    process: &ProcessRecord,
    kind: AgentKind,
    kinds: &HashMap<u32, AgentKind>,
    by_pid: &HashMap<u32, &ProcessRecord>,
) -> bool {
    let mut visited = HashSet::from([process.pid]);
    let mut current = process.ppid;
    while let Some(pid) = current {
        if !visited.insert(pid) {
            return false;
        }
        if kinds.get(&pid) == Some(&kind) {
            return true;
        }
        current = by_pid.get(&pid).and_then(|parent| parent.ppid);
    }
    false
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
