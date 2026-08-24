//! Per-thread conversation history for local turns.
//!
//! # Why this exists
//!
//! A turn's `resume_session_id` promises continuity — a bounded workflow node
//! gets a fresh thread and a resumed conversation keeps its own, matching what
//! `--session-id` buys on the CLI providers. The embedded core delivered that
//! from its own session store; without one, a resumed turn built a fresh harness
//! and sent nothing but the system prompt and the new message, so the id
//! identified a conversation that no longer existed anywhere.
//!
//! This is the replacement: the transcript a turn produced is written under the
//! thread's id and replayed on the next turn that names it.
//!
//! # Why not tinyagents' session store
//!
//! It has one, behind the `sqlite` feature this workspace already enables. It is
//! also a durable *run ledger* — sessions, tool calls, telemetry, a full-text
//! search index — and the only thing needed here is "what did we say last time".
//! A JSON file per thread is inspectable, trivially removable by an operator
//! clearing a stuck conversation, and carries no schema to migrate. If a later
//! surface needs the ledger's query side, that is the moment to adopt it.

use std::path::{Path, PathBuf};

use tinyagents::harness::message::Message;

/// Most messages replayed into a resumed turn.
///
/// A conversation grows without bound and every replayed message is prompt
/// budget spent before the model reads the new instruction. The tail is what
/// matters — recent turns carry the state a follow-up refers to — so older
/// messages are dropped rather than the whole history refused.
const MAX_REPLAYED: usize = 80;

/// The directory holding one account's thread transcripts.
fn root(home: &Path) -> PathBuf {
    home.join("agent-threads")
}

/// The file backing one thread.
///
/// The id reaches this from a workflow node, so it is not assumed to be a safe
/// filename: everything outside a conservative allowlist becomes `_`. That can
/// collide two ids onto one file, which is why the id is also recorded *inside*
/// the file and checked on read — a collision then reads as "no history" rather
/// than as another conversation's transcript.
fn location(home: &Path, thread_id: &str) -> PathBuf {
    let safe: String = thread_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(96)
        .collect();
    root(home).join(format!("{safe}.json"))
}

/// One thread's stored transcript.
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    /// The id this transcript belongs to, verbatim.
    ///
    /// Checked against the requested id on read, so a filename collision from
    /// sanitizing cannot serve one conversation's history to another.
    thread_id: String,
    /// The messages, oldest first.
    messages: Vec<Message>,
}

/// The messages to replay for `thread_id`, oldest first.
///
/// Empty for a thread with no history — a fresh node, a first turn, or a
/// transcript that cannot be read. Unreadable is deliberately not an error: the
/// turn can still run, and refusing to answer because a cache is corrupt is a
/// worse outcome than answering without the context.
pub fn load(home: &Path, thread_id: &str) -> Vec<Message> {
    let path = location(home, thread_id);
    let Ok(body) = std::fs::read(&path) else {
        return Vec::new();
    };
    let stored: Stored = match serde_json::from_slice(&body) {
        Ok(stored) => stored,
        Err(err) => {
            tracing::debug!("[agent] ignoring unreadable transcript at {path:?}: {err}");
            return Vec::new();
        }
    };
    if stored.thread_id != thread_id {
        tracing::debug!("[agent] transcript at {path:?} belongs to another thread; ignoring");
        return Vec::new();
    }

    let mut messages = stored.messages;
    if messages.len() > MAX_REPLAYED {
        messages.drain(..messages.len() - MAX_REPLAYED);
    }
    messages
}

/// Record `messages` as `thread_id`'s transcript, replacing any previous one.
///
/// Best-effort: a failure to persist is logged and swallowed. The turn has
/// already happened and its answer is already on its way to the caller, so
/// failing here would discard real work over a cache write.
pub fn save(home: &Path, thread_id: &str, messages: &[Message]) {
    // Trimmed on the way in as well as the way out, so a long-running thread's
    // file does not grow without bound just because reads cap it.
    let start = messages.len().saturating_sub(MAX_REPLAYED);
    let stored = Stored {
        thread_id: thread_id.to_string(),
        messages: messages[start..].to_vec(),
    };

    let path = location(home, thread_id);
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(err) = std::fs::create_dir_all(parent) {
        tracing::debug!("[agent] could not create {parent:?}: {err}");
        return;
    }
    match serde_json::to_vec(&stored) {
        Ok(body) => {
            if let Err(err) = std::fs::write(&path, body) {
                tracing::debug!("[agent] could not write transcript {path:?}: {err}");
            }
        }
        Err(err) => tracing::debug!("[agent] could not encode transcript: {err}"),
    }
}

#[cfg(test)]
mod tests;
