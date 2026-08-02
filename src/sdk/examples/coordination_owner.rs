//! The orchestrator side of the coordination e2e chain, ported to the host link
//! (`docs/host-link-protocol.md`).
//!
//! Its job is unchanged: send a `medulla-task/1` frame to a worker daemon, wait
//! for a terminal (`reply` / `error` / `capabilities_result`) frame, and print it
//! as one line of JSON on stdout so the shell can assert on the mock-LLM marker.
//! What changed is everything underneath. There is no relay to poll, no mailbox,
//! no directory and no pre-keys — the frame is a message on channel 0 of a link
//! whose pair key was established at enrollment, and the reply comes back on the
//! same link.
//!
//! # Modes
//!
//! - **enroll** (`--enroll`) — mint one orchestrator/host pair: a pair key, two
//!   node ids and two forwarder keys, written as `node.json` into two identity
//!   directories (§7.3). This stands in for the backend's enrollment endpoints
//!   (§7.2) plus the human who carries the pair key to the host (§7.1). The two
//!   **forwarder** keys are printed so the harness can seed the mock forwarder's
//!   node table; the pair key is never printed, because the backend never sees
//!   one and neither does anything that reads this program's output.
//! - **serve** (`--serve <dir>`) — stay up and run one leg per request file
//!   dropped into `<dir>`. This is not a convenience: the link's SSP state lives
//!   in memory, so an endpoint that exits and restarts comes back at state 0
//!   while its peer still holds state *n*, and neither side's diffs apply until
//!   both restart. A long-lived orchestrator process is what the protocol
//!   assumes, so the harness gets one instead of a process per leg.
//! - **one-shot** (neither flag) — connect, run a single leg, print the JSON,
//!   exit. Correct only against a freshly started daemon, for the reason above.
//!
//! # Flags
//!
//! Process-level:
//!
//! - `--state-dir <dir>` — this endpoint's link identity directory (`node.json`).
//!   Required in every mode.
//! - `--forwarder <host:port>` — overrides the forwarder endpoint recorded at
//!   enrollment. **Replaces `--endpoint`**, which named a tiny.place relay base
//!   URL; there is no relay and no HTTP any more.
//! - `--enroll` / `--host-state-dir <dir>` — enrollment mode and the host
//!   identity directory to write.
//! - `--serve <dir>` / `--results <dir>` — serve mode: the request queue, and
//!   where `<label>.json` and `<label>.rc` are written (default: the queue).
//!
//! Per leg (accepted on the command line in one-shot mode, and in a request file
//! in serve mode — one argument per line):
//!
//! - `--to <node-id-hex>` — the worker's **node id** (§2), not an agent handle.
//!   Kept, with a new value space: node names never travel on the wire, so the
//!   only thing that can address a peer here is its id. Checked against the peer
//!   recorded in `node.json`, since a link has exactly one.
//! - `--task <text>` — the task prompt. Unchanged.
//! - `--task-id <id>` — the task/cycle id. Unchanged, and now also the filter
//!   that decides which inbound frame terminates *this* leg.
//! - `--kind <task|capabilities>` — frame kind. Unchanged.
//! - `--provider <opencode|claude|codex>` — provider hint, for the
//!   no-available-provider error path. Unchanged.
//! - `--model <id>` — model hint. Unchanged.
//! - `--timeout-ms <n>` — how long to wait for a terminal frame (default 60000).
//!   Unchanged.
//! - `--reset-link` — serve mode only: rebuild the link before dispatching,
//!   because the peer process restarted (see the serve note above). The harness
//!   knows when it killed a daemon; the link has no in-band way to discover it.
//! - `--reset-only` — rebuild the link and finish, dispatching nothing. Used
//!   between killing a host and starting its replacement, so the frames the old
//!   session was still retransmitting cannot land on the new process and put the
//!   two ends back out of step.
//!
//! Dropped: `--endpoint` (see `--forwarder`), `--publish-only` (there are no
//! pre-keys to publish and no directory to publish them to) and the identity
//! half of `--seed` (identity now comes from `node.json`). `--seed <64hex>`
//! survives in **enroll** mode, where it makes the minted key material
//! deterministic.
//!
//! Exit code: 0 when a reply (or `capabilities_result`) arrived, 1 on an error
//! frame or a timeout, 2 on a usage or transport failure. In serve mode the same
//! code is written to `<label>.rc` per leg and the process itself runs until
//! killed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use medulla::protocol::{
    decode_task_frame, encode_task_frame, EncodeFrameInput, HarnessProvider, TaskFrame,
    TaskFrameKind,
};
use medulla_link::keys::{self, ForwarderKey, NodeId, NodeState, PairKey, Role};
use medulla_link::{Link, LinkConfig, LinkHandle};
use sha2::{Digest, Sha256};

/// How often the serve loop looks for new request files.
const POLL: Duration = Duration::from_millis(200);

/// What one leg asks for.
#[derive(Debug, Clone)]
struct Leg {
    to: Option<NodeId>,
    task: String,
    task_id: String,
    kind: TaskFrameKind,
    provider: Option<HarnessProvider>,
    model: Option<String>,
    timeout_ms: u64,
    reset_link: bool,
    reset_only: bool,
}

impl Default for Leg {
    fn default() -> Self {
        Leg {
            to: None,
            task: "print the coordination marker".to_string(),
            task_id: "coord-1".to_string(),
            kind: TaskFrameKind::Task,
            provider: None,
            model: None,
            timeout_ms: 60_000,
            reset_link: false,
            reset_only: false,
        }
    }
}

/// Everything the process itself needs, plus the leg the command line described.
#[derive(Debug)]
struct Args {
    state_dir: Option<PathBuf>,
    forwarder: Option<String>,
    enroll: bool,
    host_state_dir: Option<PathBuf>,
    serve: Option<PathBuf>,
    results: Option<PathBuf>,
    seed: Option<String>,
    leg: Leg,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            state_dir: std::env::var("MEDULLA_LINK_STATE_DIR")
                .ok()
                .map(PathBuf::from),
            forwarder: std::env::var("MEDULLA_LINK_FORWARDER").ok(),
            enroll: false,
            host_state_dir: None,
            serve: None,
            results: None,
            seed: None,
            leg: Leg::default(),
        }
    }
}

/// Parse one flag stream. Used for argv and, in serve mode, for a request file.
fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args::default();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--state-dir" => args.state_dir = Some(PathBuf::from(value()?)),
            "--forwarder" => args.forwarder = Some(value()?),
            "--enroll" => args.enroll = true,
            "--host-state-dir" => args.host_state_dir = Some(PathBuf::from(value()?)),
            "--serve" => args.serve = Some(PathBuf::from(value()?)),
            "--results" => args.results = Some(PathBuf::from(value()?)),
            "--seed" => args.seed = Some(value()?),
            "--to" => args.leg.to = Some(parse_node_id(&value()?)?),
            "--task" => args.leg.task = value()?,
            "--task-id" => args.leg.task_id = value()?,
            "--model" => args.leg.model = Some(value()?),
            "--reset-link" => args.leg.reset_link = true,
            "--reset-only" => {
                args.leg.reset_link = true;
                args.leg.reset_only = true;
            }
            "--kind" => {
                let raw = value()?;
                args.leg.kind =
                    TaskFrameKind::from_wire(&raw).ok_or(format!("unknown --kind: {raw}"))?;
            }
            "--provider" => {
                let raw = value()?;
                args.leg.provider = Some(
                    HarnessProvider::from_wire(&raw).ok_or(format!("unknown --provider: {raw}"))?,
                );
            }
            "--timeout-ms" => {
                args.leg.timeout_ms = value()?
                    .parse()
                    .map_err(|_| "--timeout-ms must be a number".to_string())?;
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(args)
}

/// Decode a 32-character hex node id.
fn parse_node_id(text: &str) -> Result<NodeId, String> {
    let bytes = decode_hex(text.trim(), 16)?;
    Ok(NodeId(bytes.try_into().expect("decode_hex checked length")))
}

/// Decode exactly `want` bytes of hex.
fn decode_hex(text: &str, want: usize) -> Result<Vec<u8>, String> {
    if text.len() != want * 2 {
        return Err(format!(
            "expected {} hex characters, got {}",
            want * 2,
            text.len()
        ));
    }
    (0..want)
        .map(|i| {
            u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                .map_err(|_| format!("not hex: {text:?}"))
        })
        .collect()
}

/// Lowercase hex, the encoding the forwarder's `--node` flag expects.
fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("coordination_owner: {err}");
            std::process::exit(2);
        }
    };
    let code = match run(args).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("coordination_owner: {err}");
            2
        }
    };
    std::process::exit(code);
}

/// Dispatch on mode, returning the process exit code.
async fn run(args: Args) -> Result<i32, String> {
    let state_dir = args
        .state_dir
        .clone()
        .ok_or("missing --state-dir (or $MEDULLA_LINK_STATE_DIR)")?;
    if args.enroll {
        enroll(&args, &state_dir)?;
        return Ok(0);
    }

    let owner_id = keys::read_node_state(&keys::node_path(&state_dir))
        .map_err(|e| format!("could not read {}: {e}", state_dir.display()))?
        .node_id;
    let link = connect(&state_dir, args.forwarder.as_deref()).await?;

    match args.serve.clone() {
        Some(queue) => {
            let results = args.results.clone().unwrap_or_else(|| queue.clone());
            serve(link, &state_dir, &args, owner_id, &queue, &results).await
        }
        None => {
            let (code, report) = run_leg(&link, owner_id, &args.leg).await;
            println!("{report}");
            Ok(code)
        }
    }
}

/// Mint an orchestrator/host pair and write both `node.json` files (§7).
///
/// The pair key is generated here, on the orchestrator, exactly as §7.1 says —
/// and written straight into the host's identity file rather than typed in by a
/// human, which is the one step of enrollment a harness cannot perform.
fn enroll(args: &Args, owner_dir: &Path) -> Result<(), String> {
    let host_dir = args
        .host_state_dir
        .clone()
        .ok_or("--enroll needs --host-state-dir")?;
    let forwarder = args
        .forwarder
        .clone()
        .ok_or("--enroll needs --forwarder <host:port>")?;

    let material = match args.seed.as_deref() {
        Some(seed) => Material::from_seed(seed)?,
        None => Material::random(),
    };

    write_identity(
        owner_dir,
        &material,
        Role::Orchestrator,
        forwarder.clone(),
        material.owner_id,
        material.host_id,
        &material.owner_key,
    )?;
    write_identity(
        &host_dir,
        &material,
        Role::Host,
        forwarder,
        material.host_id,
        material.owner_id,
        &material.host_key,
    )?;

    // The forwarder keys are the backend's half of enrollment, so they are what
    // the mock forwarder needs. The pair key stays between the two endpoints.
    println!("OWNER_NODE_ID={}", material.owner_id);
    println!(
        "OWNER_FORWARDER_KEY={}",
        encode_hex(material.owner_key.as_bytes())
    );
    println!("HOST_NODE_ID={}", material.host_id);
    println!(
        "HOST_FORWARDER_KEY={}",
        encode_hex(material.host_key.as_bytes())
    );
    Ok(())
}

/// The key material one enrollment mints.
struct Material {
    owner_id: NodeId,
    host_id: NodeId,
    owner_key: ForwarderKey,
    host_key: ForwarderKey,
    pair_key: PairKey,
}

impl Material {
    /// Fresh random material — the normal case.
    fn random() -> Self {
        Material {
            owner_id: NodeId::generate(),
            host_id: NodeId::generate(),
            owner_key: ForwarderKey::generate(),
            host_key: ForwarderKey::generate(),
            pair_key: PairKey::generate(),
        }
    }

    /// Material derived from a 64-character hex seed, so a run is reproducible.
    ///
    /// Each field is a separate SHA-256 over the seed and a label, so learning
    /// one tells an attacker nothing about the others — a test fixture is still
    /// key material.
    fn from_seed(seed: &str) -> Result<Self, String> {
        let seed = decode_hex(seed.trim(), 32)?;
        let derive = |label: &str, len: usize| -> Vec<u8> {
            let mut hasher = Sha256::new();
            hasher.update(label.as_bytes());
            hasher.update(&seed);
            hasher.finalize()[..len].to_vec()
        };
        let id =
            |label: &str| -> NodeId { NodeId(derive(label, 16).try_into().expect("16 bytes")) };
        let key = |label: &str| -> ForwarderKey {
            ForwarderKey(derive(label, 32).try_into().expect("32 bytes"))
        };
        Ok(Material {
            owner_id: id("medulla-e2e owner-node"),
            host_id: id("medulla-e2e host-node"),
            owner_key: key("medulla-e2e owner-forwarder"),
            host_key: key("medulla-e2e host-forwarder"),
            pair_key: PairKey::from_bytes(derive("medulla-e2e pair", 16).try_into().expect("16")),
        })
    }
}

/// Write one endpoint's `node.json`, creating the directory if needed.
fn write_identity(
    dir: &Path,
    material: &Material,
    role: Role,
    forwarder_endpoint: String,
    node_id: NodeId,
    peer_node_id: NodeId,
    forwarder_key: &ForwarderKey,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    keys::acquire_or_create(dir, || NodeState {
        version: 1,
        node_id,
        role,
        pair_key: material.pair_key.clone(),
        forwarder_key: forwarder_key.clone(),
        forwarder_endpoint,
        peer_node_id,
        seq_reservation: 1,
    })
    .map_err(|e| format!("could not enroll {}: {e}", dir.display()))?;
    Ok(())
}

/// Bring the link up, retrying briefly while a previous driver releases the
/// identity lock (the lock is dropped when the old driver task stops, which is
/// shortly after its handle is).
async fn connect(state_dir: &Path, forwarder: Option<&str>) -> Result<LinkHandle, String> {
    let mut config = LinkConfig::new(state_dir);
    config.forwarder_endpoint = forwarder.map(str::to_string);
    let mut last = String::new();
    for _ in 0..25 {
        match Link::connect(config.clone()).await {
            Ok(link) => return Ok(link),
            Err(err) => {
                last = err.to_string();
                tokio::time::sleep(POLL).await;
            }
        }
    }
    Err(format!(
        "could not bring up the link at {}: {last}",
        state_dir.display()
    ))
}

/// Run one leg: send the frame, wait for the terminal frame, build the report.
///
/// Returns the leg's exit code and the JSON line describing it.
async fn run_leg(link: &LinkHandle, owner_id: NodeId, leg: &Leg) -> (i32, serde_json::Value) {
    let Some(peer) = leg.to.or_else(|| link.peers().first().copied()) else {
        return (
            2,
            report_error(owner_id, leg, "missing --to <worker node id>"),
        );
    };
    if !link.peers().contains(&peer) {
        let known: Vec<String> = link.peers().iter().map(NodeId::to_string).collect();
        return (
            2,
            report_error(
                owner_id,
                leg,
                &format!(
                    "{peer} is not this link's peer (enrolled: {})",
                    known.join(", ")
                ),
            ),
        );
    }

    let frame = encode_task_frame(EncodeFrameInput {
        kind: leg.kind,
        task_id: leg.task_id.clone(),
        text: leg.task.clone(),
        ts: medulla::clock::iso_now(),
        correlation_id: Some(format!("{}-corr", leg.task_id)),
        harness: None,
        provider: leg.provider,
        custom_harness: None,
        model: leg.model.clone(),
        tool_mode: None,
        workflow: None,
        conversation: None,
    });
    if let Err(err) = link.send(peer, frame.as_bytes()).await {
        return (
            2,
            report_error(owner_id, leg, &format!("send failed: {err}")),
        );
    }
    eprintln!(
        "coordination_owner: {owner_id} → {peer} task {} sent, waiting for a terminal frame…",
        leg.task_id
    );

    let deadline = tokio::time::Instant::now() + Duration::from_millis(leg.timeout_ms);
    let mut collected: Vec<TaskFrame> = Vec::new();
    let mut terminal: Option<TaskFrame> = None;
    while terminal.is_none() {
        let Ok(Some((_from, body))) = tokio::time::timeout_at(deadline, link.recv()).await else {
            break; // the deadline passed, or the link closed under us
        };
        let text = String::from_utf8_lossy(&body).into_owned();
        let Some(frame) = decode_task_frame(&text) else {
            eprintln!("coordination_owner: ignoring a message that is not a task frame");
            continue;
        };
        // A leg only ends on its *own* task. A reply to an earlier, timed-out leg
        // can still be in flight on a link this process keeps across legs, and
        // letting it terminate this one would report the wrong answer.
        if frame.task_id != leg.task_id {
            eprintln!(
                "coordination_owner: ignoring frame for task {} (waiting on {})",
                frame.task_id, leg.task_id
            );
            continue;
        }
        eprintln!(
            "coordination_owner: frame kind={:?} text={:?}",
            frame.kind, frame.text
        );
        let is_terminal = matches!(
            frame.kind,
            TaskFrameKind::Reply | TaskFrameKind::Error | TaskFrameKind::CapabilitiesResult
        );
        collected.push(frame.clone());
        if is_terminal {
            terminal = Some(frame);
        }
    }

    match terminal {
        Some(frame) => {
            let code = i32::from(!matches!(
                frame.kind,
                TaskFrameKind::Reply | TaskFrameKind::CapabilitiesResult
            ));
            (code, report(owner_id, &frame, &collected))
        }
        None => {
            eprintln!(
                "coordination_owner: timed out with no terminal frame ({} frames seen)",
                collected.len()
            );
            (
                1,
                report_error(owner_id, leg, "timed out with no terminal frame"),
            )
        }
    }
}

/// The terminal frame as the JSON line the shell asserts on.
fn report(owner_id: NodeId, frame: &TaskFrame, collected: &[TaskFrame]) -> serde_json::Value {
    serde_json::json!({
        "kind": format!("{:?}", frame.kind),
        "text": frame.text,
        "taskId": frame.task_id,
        "correlationId": frame.correlation_id,
        "harness": frame.harness.map(|h| h.as_str().to_string()),
        "ownerId": owner_id.to_string(),
        "frames": collected.len(),
        "frameKinds": collected.iter().map(|f| f.kind.as_str().to_string()).collect::<Vec<_>>(),
        "usage": frame.usage.as_ref().map(|u| serde_json::json!({
            "inputTokens": u.input_tokens,
            "outputTokens": u.output_tokens,
        })),
    })
}

/// The same shape for a leg that never reached a terminal frame, so a scenario
/// asserting on the JSON sees a reason rather than an empty file.
fn report_error(owner_id: NodeId, leg: &Leg, reason: &str) -> serde_json::Value {
    eprintln!("coordination_owner: {reason}");
    serde_json::json!({
        "kind": "None",
        "text": reason,
        "taskId": leg.task_id,
        "ownerId": owner_id.to_string(),
        "frames": 0,
        "frameKinds": Vec::<String>::new(),
    })
}

/// Run legs from a request queue until the process is killed.
///
/// A request is a file `<label>.req` holding one argument per line. The harness
/// writes it under a temporary name and renames it into place, so this loop
/// never reads a half-written file. Each finished leg writes `<label>.json` (the
/// terminal-frame report) and then `<label>.rc` (the exit code) into the results
/// directory — in that order, because the harness waits on the `.rc`.
async fn serve(
    connected: LinkHandle,
    state_dir: &Path,
    args: &Args,
    owner_id: NodeId,
    queue: &Path,
    results: &Path,
) -> Result<i32, String> {
    std::fs::create_dir_all(queue)
        .map_err(|e| format!("could not create {}: {e}", queue.display()))?;
    std::fs::create_dir_all(results)
        .map_err(|e| format!("could not create {}: {e}", results.display()))?;
    println!(
        "coordination_owner serving {owner_id} from {}",
        queue.display()
    );
    // Held in an `Option` so a rebuild can drop the old link *before* opening the
    // new one: the identity lock (§7.3) is exclusive, and a reassignment would
    // hold both at once and deadlock against itself.
    let mut link = Some(connected);

    loop {
        for (label, path) in pending(queue)? {
            let taken = path.with_extension("taken");
            if std::fs::rename(&path, &taken).is_err() {
                continue; // another pass took it, or it vanished
            }
            let body = std::fs::read_to_string(&taken)
                .map_err(|e| format!("could not read {}: {e}", taken.display()))?;
            let lines = body
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let request = match parse_args(lines.into_iter()) {
                Ok(request) => request,
                Err(err) => {
                    let report = serde_json::json!({ "kind": "None", "text": err });
                    write_result(results, &label, 2, &report)?;
                    continue;
                }
            };
            if request.leg.reset_link {
                // The peer restarted, so its SSP state is back at 0 while ours is
                // not. Rebuilding the link puts both ends back on a shared origin;
                // the persisted sequence reservation (§3.1) means no nonce repeats.
                eprintln!("coordination_owner: rebuilding the link for leg {label}");
                drop(link.take());
                link = Some(connect(state_dir, args.forwarder.as_deref()).await?);
            }
            if request.leg.reset_only {
                let report = serde_json::json!({
                    "kind": "LinkReset",
                    "text": "link rebuilt; nothing dispatched",
                    "taskId": request.leg.task_id,
                    "ownerId": owner_id.to_string(),
                });
                println!("{report}");
                write_result(results, &label, 0, &report)?;
                continue;
            }
            let handle = link.as_ref().expect("the link is only ever briefly absent");
            let (code, report) = run_leg(handle, owner_id, &request.leg).await;
            println!("{report}");
            write_result(results, &label, code, &report)?;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Request files waiting in the queue, oldest name first.
fn pending(queue: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let entries =
        std::fs::read_dir(queue).map_err(|e| format!("could not read {}: {e}", queue.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("req") {
            continue;
        }
        let Some(label) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        out.push((label.to_string(), path.clone()));
    }
    out.sort();
    Ok(out)
}

/// Write `<label>.json` then `<label>.rc`.
fn write_result(
    results: &Path,
    label: &str,
    code: i32,
    report: &serde_json::Value,
) -> Result<(), String> {
    let json = results.join(format!("{label}.json"));
    std::fs::write(&json, format!("{report}\n"))
        .map_err(|e| format!("could not write {}: {e}", json.display()))?;
    let rc = results.join(format!("{label}.rc"));
    std::fs::write(&rc, format!("{code}\n"))
        .map_err(|e| format!("could not write {}: {e}", rc.display()))
}
