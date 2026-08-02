//! Remote bridge backed by the medulla host link (`docs/host-link-protocol.md`).
//!
//! This is what replaced the tiny.place mailbox. The shape is the same one the
//! mailbox bridge had — a `Bridge` over a transport — but almost everything the
//! trait asks for collapses, because a state-synchronising link does not have
//! the problems a store-and-forward relay had:
//!
//! - **No handshake.** [`request_contact`](Bridge::request_contact) succeeds
//!   without doing anything and [`contact_accepted`](Bridge::contact_accepted)
//!   is always true: enrollment already established the pair key, so the first
//!   datagram is the first message.
//! - **No addresses to resolve.** A worker's address *is* its node name, issued
//!   at enrollment and stable, so [`resolve_handle`](Bridge::resolve_handle)
//!   answers from the peer table rather than a directory lookup.
//! - **No session to reset.** See [`LinkBridge::reset_session`].
//!
//! What does not collapse is the inbox. A pump task drains
//! [`LinkHandle::recv`] into a queue plus a [`Notify`], so
//! [`drain_inbox`](Bridge::drain_inbox) and
//! [`wait_for_inbox`](Bridge::wait_for_inbox) keep exactly the semantics the
//! mailbox bridge gave them and `TaskRunner` needs no change. That equivalence
//! is what makes the transport swap safe.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use medulla_link::keys::NodeId;
use medulla_link::{LinkHandle, Liveness, MAX_MESSAGE_BYTES};

use super::{Bridge, BridgeLiveness, InboundMessage};

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;

/// Maximum number of complete frames buffered above the bounded link channel.
const INBOX_CAPACITY: usize = 1024;

const CHUNK_MAGIC: &[u8; 4] = b"MDL1";
const CHUNK_HEADER: usize = 20;
const MAX_CHUNKS: usize = 4096;
const MAX_PARTIAL_MESSAGES: usize = 128;
const MAX_PARTIAL_BYTES: usize = 16 * 1024 * 1024;
const PARTIAL_TTL: Duration = Duration::from_secs(30);

/// Mint an id that does not reset to a predictable value with the process.
fn next_message_id() -> u64 {
    u64::from_be_bytes(Uuid::new_v4().as_bytes()[..8].try_into().expect("8 bytes"))
}

struct PartialMessage {
    chunks: Vec<Option<Vec<u8>>>,
    bytes: usize,
    updated: Instant,
}

#[derive(Default)]
struct Reassembly {
    partials: HashMap<(NodeId, u64), PartialMessage>,
    bytes: usize,
}

impl Reassembly {
    /// Drop incomplete messages that have stopped making progress.
    ///
    /// Only expires partials when the link is known to be Live. During outages
    /// (Degraded/Offline), partials are retained so they can complete once the
    /// peer reconnects within the timeout window. This aligns with task-layer
    /// clock pausing during outages (see TaskRunner's ACK_WINDOW/IDLE_WINDOW).
    fn expire(&mut self, now: Instant, is_live: bool) {
        if !is_live {
            // Pause expiry during outages so the timeout window extends across recovery.
            return;
        }
        let stale: Vec<_> = self
            .partials
            .iter()
            .filter_map(|(key, partial)| {
                (now.saturating_duration_since(partial.updated) >= PARTIAL_TTL).then_some(*key)
            })
            .collect();
        for key in stale {
            self.remove(&key);
        }
    }

    /// Evict the least recently updated message other than `protected`.
    fn evict_oldest_except(&mut self, protected: Option<(NodeId, u64)>) -> bool {
        let oldest = self
            .partials
            .iter()
            .filter(|(key, _)| protected.as_ref() != Some(*key))
            .min_by_key(|(_, partial)| partial.updated)
            .map(|(key, _)| *key);
        if let Some(key) = oldest {
            self.remove(&key);
            true
        } else {
            false
        }
    }

    fn remove(&mut self, key: &(NodeId, u64)) -> Option<PartialMessage> {
        let partial = self.partials.remove(key)?;
        self.bytes = self.bytes.saturating_sub(partial.bytes);
        Some(partial)
    }
}

/// One reachable peer: the name callers address it by, and the id on the wire.
///
/// The two are separate on purpose (protocol §2). `node_name` is human-readable
/// and unique within a team; `node_id` is 16 random bytes and the only
/// identifier that travels. Endpoints resolve name → id once, at enrollment.
#[derive(Debug, Clone)]
pub struct LinkPeer {
    /// The bridge address — what the roster, the hub and the TUI use.
    pub name: String,
    /// The wire identifier this bridge sends to.
    pub node_id: NodeId,
}

/// How to build a [`LinkBridge`] over an already-connected link.
#[derive(Debug, Clone)]
pub struct LinkBridgeConfig {
    /// This endpoint's own node name — the address peers send to.
    pub node_name: String,
    /// Every peer this endpoint can address.
    pub peers: Vec<LinkPeer>,
}

/// The queue the pump fills and `drain_inbox` empties.
///
/// Split out from [`LinkBridge`] so the pump task can hold it without holding
/// the bridge, which would keep the bridge alive forever and never let the pump
/// stop.
#[derive(Default)]
struct Inbox {
    queue: Mutex<VecDeque<InboundMessage>>,
    reassembly: Mutex<Reassembly>,
    /// Notified on every delivery, so `wait_for_inbox` returns at about a round
    /// trip rather than at the caller's poll interval.
    arrival: Notify,
    /// Wakes the pump after a consumer frees queue capacity.
    space: Notify,
}

/// Shared state behind every clone of a [`LinkBridge`].
struct Inner {
    link: Arc<LinkHandle>,
    address: String,
    /// Address → wire id, for sending.
    ids: HashMap<String, NodeId>,
    /// Wire id → address, for naming the sender of an inbound message.
    names: HashMap<NodeId, String>,
    inbox: Arc<Inbox>,
    pump: tokio::task::JoinHandle<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.pump.abort();
    }
}

/// A remote bridge that delivers over the host link.
#[derive(Clone)]
pub struct LinkBridge {
    inner: Arc<Inner>,
}

impl LinkBridge {
    /// Wrap a connected link, starting the inbox pump.
    ///
    /// The pump runs until every clone of the bridge is dropped. Messages that
    /// arrive before anything drains are queued, exactly as the mailbox held
    /// them, so a consumer that starts late does not lose work.
    pub fn new(link: Arc<LinkHandle>, config: LinkBridgeConfig) -> Self {
        let mut ids = HashMap::new();
        let mut names = HashMap::new();
        for peer in config.peers {
            names.insert(peer.node_id, peer.name.clone());
            ids.insert(peer.name, peer.node_id);
        }
        let inbox = Arc::new(Inbox::default());
        let pump = tokio::spawn(pump(link.clone(), names.clone(), inbox.clone()));
        LinkBridge {
            inner: Arc::new(Inner {
                link,
                address: config.node_name,
                ids,
                names,
                inbox,
                pump,
            }),
        }
    }

    /// Wrap a link whose peer table came from `node.json` — the host case,
    /// which has exactly one peer (protocol §7.3).
    ///
    /// # Errors
    ///
    /// When the link did not open exactly one session, because then there is no
    /// unambiguous peer to give `peer_name` to.
    pub fn single_peer(
        link: Arc<LinkHandle>,
        node_name: impl Into<String>,
        peer_name: impl Into<String>,
    ) -> Result<Self, String> {
        let peers = link.peers();
        let [node_id] = peers else {
            return Err(format!(
                "expected the link to have exactly one peer, found {}",
                peers.len()
            ));
        };
        let node_id = *node_id;
        Ok(LinkBridge::new(
            link,
            LinkBridgeConfig {
                node_name: node_name.into(),
                peers: vec![LinkPeer {
                    name: peer_name.into(),
                    node_id,
                }],
            },
        ))
    }

    /// This endpoint's node name.
    pub fn address(&self) -> &str {
        &self.inner.address
    }

    /// The underlying link, for status displays and screen streaming.
    pub fn link(&self) -> &Arc<LinkHandle> {
        &self.inner.link
    }

    /// The wire id for a peer address, if this endpoint knows one.
    pub fn node_id(&self, address: &str) -> Option<NodeId> {
        self.inner.ids.get(address.trim()).copied()
    }

    /// Every peer address this bridge can reach.
    pub fn peer_names(&self) -> impl Iterator<Item = &str> {
        self.inner.names.values().map(String::as_str)
    }
}

/// Drain the link into the bridge's inbox until the link stops.
async fn pump(link: Arc<LinkHandle>, names: HashMap<NodeId, String>, inbox: Arc<Inbox>) {
    while let Some((peer, body)) = link.recv().await {
        let status = link.status();
        let is_live = status
            .peer(&peer)
            .is_some_and(|peer| peer.liveness == Liveness::Live);
        let Some(body) = reassemble_with_liveness(peer, body, is_live, &inbox).await else {
            continue;
        };
        // An unknown sender is named by its id rather than dropped: the message
        // authenticated under a pair key we hold, so it is genuinely from a peer
        // this endpoint is enrolled with — only the local name table is behind.
        let from = names
            .get(&peer)
            .cloned()
            .unwrap_or_else(|| peer.to_string());
        // Bodies are UTF-8 by construction (every caller sends `&str`), but a
        // lossy decode is the right failure: a mangled frame surfaces as a frame
        // that does not parse, not as a silently vanished message.
        let text = String::from_utf8_lossy(&body).into_owned();
        let mut message = Some(InboundMessage { from, text });
        loop {
            let space = inbox.space.notified();
            {
                let mut queue = inbox.queue.lock().await;
                if queue.len() < INBOX_CAPACITY {
                    queue.push_back(message.take().expect("message is queued once"));
                    inbox.arrival.notify_waiters();
                    break;
                }
            }
            space.await;
        }
    }
}

/// Decode one bridge fragment, returning a complete application frame once all
/// chunks have arrived. Link channel 0 is ordered, but the id keeps concurrent
/// callers independent when their chunk sequences interleave.
///
/// Only expires partial message buffers when the peer's link is Live; during
/// outages (Degraded/Offline), partials are retained to complete once connectivity
/// recovers, aligning with task-layer clock pausing for peer outages.
#[cfg(test)]
async fn reassemble(peer: NodeId, body: Vec<u8>, inbox: &Inbox) -> Option<Vec<u8>> {
    reassemble_with_liveness(peer, body, true, inbox).await
}

/// Reassemble a fragment while applying the caller's current peer-liveness
/// observation to partial-message expiry.
async fn reassemble_with_liveness(
    peer: NodeId,
    body: Vec<u8>,
    is_live: bool,
    inbox: &Inbox,
) -> Option<Vec<u8>> {
    if body.len() < CHUNK_HEADER || &body[..4] != CHUNK_MAGIC {
        return Some(body);
    }
    let id = u64::from_be_bytes(body[4..12].try_into().ok()?);
    let index = u32::from_be_bytes(body[12..16].try_into().ok()?) as usize;
    let count = u32::from_be_bytes(body[16..20].try_into().ok()?) as usize;
    if count == 0 || count > MAX_CHUNKS || index >= count {
        return None;
    }
    let key = (peer, id);
    let now = Instant::now();
    let mut reassembly = inbox.reassembly.lock().await;
    reassembly.expire(now, is_live);
    if !reassembly.partials.contains_key(&key) {
        while reassembly.partials.len() >= MAX_PARTIAL_MESSAGES {
            if !reassembly.evict_oldest_except(None) {
                return None;
            }
        }
        reassembly.partials.insert(
            key,
            PartialMessage {
                chunks: (0..count).map(|_| None).collect(),
                bytes: 0,
                updated: now,
            },
        );
    }
    if reassembly.partials[&key].chunks.len() != count {
        reassembly.remove(&key);
        return None;
    }

    let payload = &body[CHUNK_HEADER..];
    let old_len = reassembly.partials[&key].chunks[index]
        .as_ref()
        .map_or(0, Vec::len);
    let added = payload.len().saturating_sub(old_len);
    while reassembly.bytes.saturating_add(added) > MAX_PARTIAL_BYTES {
        if !reassembly.evict_oldest_except(Some(key)) {
            reassembly.remove(&key);
            return None;
        }
    }
    let partial = reassembly.partials.get_mut(&key).expect("partial exists");
    partial.bytes = partial.bytes + payload.len() - old_len;
    partial.updated = now;
    partial.chunks[index] = Some(payload.to_vec());
    if payload.len() >= old_len {
        reassembly.bytes += added;
    } else {
        reassembly.bytes -= old_len - payload.len();
    }
    if reassembly.partials[&key].chunks.iter().any(Option::is_none) {
        return None;
    }
    let partial = reassembly.remove(&key).expect("partial exists");
    Some(partial.chunks.into_iter().flatten().flatten().collect())
}

#[async_trait]
impl Bridge for LinkBridge {
    /// Queue `body` for `to`.
    ///
    /// Returns once the frame is in the outbound state, not once it is
    /// delivered: the link owns retransmission, so there is no ack window at
    /// this layer and no mailbox to push to.
    async fn send(&self, to: &str, body: &str) -> Result<(), String> {
        let peer = self
            .node_id(to)
            .ok_or_else(|| format!("no link peer is enrolled for address: {to}"))?;
        let payload = body.as_bytes();
        let capacity = MAX_MESSAGE_BYTES - CHUNK_HEADER;
        let count = payload.len().div_ceil(capacity).max(1);
        if count > MAX_CHUNKS {
            return Err(format!(
                "bridge frame requires {count} chunks; limit is {MAX_CHUNKS}"
            ));
        }
        // Process-local counters collide after a sender restart while the peer
        // may still retain an old incomplete message. A random v4 UUID prefix
        // makes the 64-bit wire id restart-unique without changing the fragment
        // header or coupling this layer to the transport's private epoch.
        let id = next_message_id();
        for (index, chunk) in payload.chunks(capacity).enumerate() {
            let mut frame = Vec::with_capacity(CHUNK_HEADER + chunk.len());
            frame.extend_from_slice(CHUNK_MAGIC);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&(index as u32).to_be_bytes());
            frame.extend_from_slice(&(count as u32).to_be_bytes());
            frame.extend_from_slice(chunk);
            send_with_backpressure(&self.inner.link, peer, &frame).await?;
        }
        // `chunks()` yields no item for an empty payload.
        if payload.is_empty() {
            let mut frame = Vec::with_capacity(CHUNK_HEADER);
            frame.extend_from_slice(CHUNK_MAGIC);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&0u32.to_be_bytes());
            frame.extend_from_slice(&1u32.to_be_bytes());
            send_with_backpressure(&self.inner.link, peer, &frame).await?;
        }
        Ok(())
    }

    async fn drain_inbox(&self, limit: i64) -> Vec<InboundMessage> {
        if limit <= 0 {
            return Vec::new();
        }
        let mut queue = self.inner.inbox.queue.lock().await;
        let count = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .min(queue.len());
        let drained = queue.drain(..count).collect();
        if count > 0 {
            self.inner.inbox.space.notify_waiters();
        }
        drained
    }

    /// Return as soon as the pump delivers, or after `poll`.
    ///
    /// The timeout stays the correctness floor: a delivery is an optimisation,
    /// never the only way something is learned.
    async fn wait_for_inbox(&self, poll: std::time::Duration) {
        if !self.inner.inbox.queue.lock().await.is_empty() {
            return;
        }
        let arrival = self.inner.inbox.arrival.notified();
        tokio::select! {
            _ = arrival => {}
            _ = tokio::time::sleep(poll) => {}
        }
    }

    /// No-op: enrollment is the only handshake the link has.
    ///
    /// The relay refused a message between non-contacts, so a sender had to ask
    /// first and wait. Here the pair key established at enrollment *is* the
    /// permission, and there is no edge to create.
    async fn request_contact(&self, _peer: &str) -> Result<(), String> {
        Ok(())
    }

    /// A node name is already the address, so this only checks it is enrolled.
    async fn resolve_handle(&self, name: &str) -> Option<String> {
        let name = name.trim().trim_start_matches('@');
        self.inner.ids.contains_key(name).then(|| name.to_string())
    }

    /// Always true: an enrolled peer is reachable by definition.
    ///
    /// Note this says nothing about whether the peer is *up* — that is
    /// [`liveness`](Bridge::liveness), and the two must not be conflated. A peer
    /// that is offline is still a peer we are permitted (and still trying) to
    /// send to.
    async fn contact_accepted(&self, _peer: &str) -> bool {
        true
    }

    /// No-op, deliberately.
    ///
    /// The mailbox bridge reset a Signal session because a restarted peer left
    /// our ratchet one-sided and every subsequent ciphertext undecryptable — the
    /// only escape was to tear the session down and re-run X3DH. SSP has no such
    /// state: a diff the peer cannot apply is simply refused, the next `ack_num`
    /// tells us where the peer actually is, and we re-diff from there. Tearing
    /// the session down here would instead discard the queued frames the link is
    /// still trying to deliver, and leave the peer holding a state number this
    /// side no longer has — turning a recoverable outage into a broken link.
    async fn reset_session(&self, _peer: &str) {}

    /// Reachability, derived from link liveness (§6.2) rather than asserted by
    /// a directory.
    ///
    /// `Degraded` still counts as reachable: the link is retransmitting and the
    /// peer has not failed, so taking it out of the roster would hide a host
    /// that is about to answer. Only sustained silence reads as offline. An
    /// address this endpoint is not enrolled with gets no entry at all — "no
    /// answer", which callers must not read as "offline".
    async fn presence(&self, addresses: &[String]) -> std::collections::HashMap<String, bool> {
        let mut out = std::collections::HashMap::new();
        for address in addresses {
            let Some(node_id) = self.node_id(address) else {
                continue;
            };
            let Some(status) = self.inner.link.status().peer(&node_id).map(|p| p.liveness) else {
                continue;
            };
            out.insert(
                address.clone(),
                matches!(status, Liveness::Live | Liveness::Degraded),
            );
        }
        out
    }

    async fn liveness(&self, peer: &str) -> BridgeLiveness {
        let Some(node_id) = self.node_id(peer) else {
            // Not enrolled: nothing is being retransmitted, so nothing is
            // pending recovery and a caller's clock should keep running.
            return BridgeLiveness::Live;
        };
        match self.inner.link.status().peer(&node_id) {
            Some(status) => match status.liveness {
                Liveness::Live => BridgeLiveness::Live,
                Liveness::Degraded => BridgeLiveness::Degraded,
                Liveness::Offline => BridgeLiveness::Offline,
            },
            None => BridgeLiveness::Live,
        }
    }
}

/// Retain one fragment and its message id while link history is full.
async fn send_with_backpressure(
    link: &LinkHandle,
    peer: NodeId,
    frame: &[u8],
) -> Result<(), String> {
    loop {
        match link.send(peer, frame).await {
            Ok(()) => return Ok(()),
            Err(err) if err.is_retryable() => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}
