//! Recovery tests for bidirectional delivery and endpoint restarts.

use super::*;

#[test]
fn a_bidirectional_task_round_trip_delivers_the_terminal_frame() {
    let mut pair = Pair::new();
    pair.orchestrator.queue_message(b"task".to_vec()).unwrap();
    let task = pair
        .orchestrator
        .outgoing(100, &mut pair.orchestrator_seq)
        .unwrap();
    pair.host.handle_datagram(&task[0], 110).unwrap();
    assert_eq!(pair.host.take_messages(), vec![b"task".to_vec()]);

    pair.host.queue_message(b"status".to_vec()).unwrap();
    let status = pair.host.outgoing(120, &mut pair.host_seq).unwrap();
    pair.orchestrator.handle_datagram(&status[0], 130).unwrap();
    assert_eq!(pair.orchestrator.take_messages(), vec![b"status".to_vec()]);

    pair.host.queue_message(b"terminal".to_vec()).unwrap();
    let terminal = pair.host.outgoing(140, &mut pair.host_seq).unwrap();
    pair.orchestrator
        .handle_datagram(&terminal[0], 150)
        .unwrap();
    let ack = pair
        .orchestrator
        .outgoing(160, &mut pair.orchestrator_seq)
        .unwrap();
    pair.host.handle_datagram(&ack[0], 170).unwrap();
    let retry = pair.host.outgoing(1_000, &mut pair.host_seq).unwrap();
    pair.orchestrator.handle_datagram(&retry[0], 190).unwrap();
    assert_eq!(
        pair.orchestrator.take_messages(),
        vec![b"terminal".to_vec()]
    );
}

#[test]
fn one_endpoint_restart_rebases_the_live_peer_and_delivers_new_work() {
    let mut pair = Pair::new();
    pair.orchestrator.queue_message(b"before".to_vec()).unwrap();
    let first = pair
        .orchestrator
        .outgoing(100, &mut pair.orchestrator_seq)
        .unwrap();
    pair.host.handle_datagram(&first[0], 110).unwrap();
    assert_eq!(pair.host.take_messages(), vec![b"before".to_vec()]);
    let ack = pair.host.outgoing(140, &mut pair.host_seq).unwrap();
    pair.orchestrator.handle_datagram(&ack[0], 150).unwrap();

    pair.host = Session::new(
        SessionConfig::new(
            NodeId([2u8; 16]),
            NodeId([1u8; 16]),
            Role::Host,
            PairKey::from_bytes([5u8; 16]),
            ForwarderKey([8u8; 32]),
        ),
        200,
    );
    let hello = pair.host.outgoing(200, &mut pair.host_seq).unwrap();
    pair.orchestrator.handle_datagram(&hello[0], 210).unwrap();
    assert!(!pair.orchestrator.handle_datagram(&ack[0], 215).unwrap());

    pair.orchestrator.queue_message(b"after".to_vec()).unwrap();
    let after = pair
        .orchestrator
        .outgoing(220, &mut pair.orchestrator_seq)
        .unwrap();
    for datagram in after {
        pair.host.handle_datagram(&datagram, 230).unwrap();
    }
    assert_eq!(pair.host.take_messages(), vec![b"after".to_vec()]);
}

#[test]
fn a_peer_restart_preserves_every_pending_message_on_channel_zero() {
    let mut pair = Pair::new();
    pair.orchestrator.queue_message(b"status".to_vec()).unwrap();
    pair.orchestrator
        .queue_message(b"terminal".to_vec())
        .unwrap();

    pair.host = Session::new(
        SessionConfig::new(
            NodeId([2u8; 16]),
            NodeId([1u8; 16]),
            Role::Host,
            PairKey::from_bytes([5u8; 16]),
            ForwarderKey([8u8; 32]),
        ),
        200,
    );
    let hello = pair.host.outgoing(200, &mut pair.host_seq).unwrap();
    pair.orchestrator.handle_datagram(&hello[0], 210).unwrap();

    let pending = pair
        .orchestrator
        .outgoing(220, &mut pair.orchestrator_seq)
        .unwrap();
    for datagram in pending {
        pair.host.handle_datagram(&datagram, 230).unwrap();
    }
    assert_eq!(
        pair.host.take_messages(),
        vec![b"status".to_vec(), b"terminal".to_vec()]
    );
}
