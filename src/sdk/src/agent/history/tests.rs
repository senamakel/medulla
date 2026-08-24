//! Transcript round-tripping, the replay bound, and the collision guard.

use tinyagents::harness::message::Message;

use super::{load, save, MAX_REPLAYED};

fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn a_thread_with_no_history_replays_nothing() {
    let dir = home();
    assert!(load(dir.path(), "medulla-fresh").is_empty());
}

#[test]
fn a_saved_transcript_comes_back_in_order() {
    let dir = home();
    let messages = vec![
        Message::user("first"),
        Message::assistant("second"),
        Message::user("third"),
    ];
    save(dir.path(), "t-1", &messages);

    let loaded = load(dir.path(), "t-1");
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].text(), "first");
    assert_eq!(loaded[2].text(), "third");
}

/// Every replayed message is prompt budget spent before the model reads the new
/// instruction, so a long conversation replays its tail rather than all of it.
#[test]
fn a_long_transcript_replays_only_its_tail() {
    let dir = home();
    let messages: Vec<Message> = (0..MAX_REPLAYED + 40)
        .map(|i| Message::user(format!("m{i}")))
        .collect();
    save(dir.path(), "t-long", &messages);

    let loaded = load(dir.path(), "t-long");
    assert_eq!(loaded.len(), MAX_REPLAYED);
    assert_eq!(
        loaded.last().unwrap().text(),
        format!("m{}", MAX_REPLAYED + 39),
        "the newest message must survive"
    );
}

/// Trimmed on write too, or a thread's file grows forever just because reads
/// happen to cap it.
#[test]
fn the_stored_file_is_trimmed_not_just_the_replay() {
    let dir = home();
    let messages: Vec<Message> = (0..MAX_REPLAYED * 3)
        .map(|i| Message::user(format!("m{i}")))
        .collect();
    save(dir.path(), "t-big", &messages);

    let path = dir.path().join("agent-threads").join("t-big.json");
    let body = std::fs::read(&path).unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        decoded["messages"].as_array().unwrap().len(),
        MAX_REPLAYED,
        "the file itself must be bounded"
    );
}

/// Sanitizing an id for use as a filename can map two ids onto one file. The id
/// is recorded inside and checked on read, so a collision reads as "no history"
/// rather than as another conversation's transcript.
#[test]
fn a_filename_collision_does_not_serve_another_threads_history() {
    let dir = home();
    save(dir.path(), "a/b", &[Message::user("private")]);

    // `a/b` and `a:b` both sanitize to `a_b`.
    let loaded = load(dir.path(), "a:b");
    assert!(
        loaded.is_empty(),
        "a colliding id must not read another thread's messages"
    );
    // The original id still reads its own.
    assert_eq!(load(dir.path(), "a/b").len(), 1);
}

/// A corrupt cache must not fail the turn: answering without context beats
/// refusing to answer.
#[test]
fn an_unreadable_transcript_reads_as_no_history() {
    let dir = home();
    let path = dir.path().join("agent-threads");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("t-bad.json"), b"{ not json").unwrap();
    assert!(load(dir.path(), "t-bad").is_empty());
}
