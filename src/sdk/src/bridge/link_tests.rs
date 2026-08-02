//! Unit tests for bridge-level fragmentation and reassembly.

use super::*;

fn chunk(id: u64, index: u32, count: u32, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(CHUNK_MAGIC);
    frame.extend_from_slice(&id.to_be_bytes());
    frame.extend_from_slice(&index.to_be_bytes());
    frame.extend_from_slice(&count.to_be_bytes());
    frame.extend_from_slice(body);
    frame
}

#[tokio::test]
async fn fragments_reassemble_without_mixing_interleaved_messages() {
    let inbox = Inbox::default();
    let peer = NodeId([7; 16]);

    assert!(reassemble(peer, chunk(1, 0, 2, b"large "), &inbox)
        .await
        .is_none());
    assert!(reassemble(peer, chunk(2, 0, 2, b"other "), &inbox)
        .await
        .is_none());
    assert_eq!(
        reassemble(peer, chunk(1, 1, 2, b"frame"), &inbox).await,
        Some(b"large frame".to_vec())
    );
    assert_eq!(
        reassemble(peer, chunk(2, 1, 2, b"message"), &inbox).await,
        Some(b"other message".to_vec())
    );
}

#[tokio::test]
async fn unframed_payloads_remain_compatible() {
    let inbox = Inbox::default();
    let body = b"legacy frame".to_vec();
    assert_eq!(
        reassemble(NodeId([8; 16]), body.clone(), &inbox).await,
        Some(body)
    );
}

#[tokio::test]
async fn incomplete_reassembly_is_bounded_and_evicts_the_oldest() {
    let inbox = Inbox::default();
    let peer = NodeId([9; 16]);
    for id in 0..=MAX_PARTIAL_MESSAGES as u64 {
        assert!(reassemble(peer, chunk(id, 0, 2, b"partial"), &inbox)
            .await
            .is_none());
    }
    assert_eq!(
        inbox.reassembly.lock().await.partials.len(),
        MAX_PARTIAL_MESSAGES
    );
    assert!(reassemble(peer, chunk(999, 0, 2, b"fresh"), &inbox)
        .await
        .is_none());
    let state = inbox.reassembly.lock().await;
    assert_eq!(state.partials.len(), MAX_PARTIAL_MESSAGES);
    assert!(state.partials.contains_key(&(peer, 999)));
}

#[tokio::test]
async fn incomplete_reassembly_survives_a_long_gap() {
    let inbox = Inbox::default();
    let peer = NodeId([10; 16]);
    assert!(
        reassemble(peer, chunk(1, 0, 2, b"held "), &inbox)
            .await
            .is_none()
    );
    inbox
        .reassembly
        .lock()
        .await
        .partials
        .get_mut(&(peer, 1))
        .expect("partial exists")
        .updated = Instant::now() - Duration::from_secs(3_600);

    assert_eq!(
        reassemble(peer, chunk(1, 1, 2, b"frame"), &inbox).await,
        Some(b"held frame".to_vec())
    );
}

#[test]
fn fragment_message_ids_are_random_across_calls() {
    assert_ne!(next_message_id(), next_message_id());
}
