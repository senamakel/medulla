//! Splitting a session list that is too large for one transport message into
//! [`RemoteMessage::SessionsChunk`] parts.

use medulla::protocol::{encode_remote_message, RemoteMessage, RemoteSessionRow};

/// The messages that carry `rows`: one whole list when it fits a datagram,
/// otherwise parts sharing `batch_id`.
pub(super) fn sessions_messages(rows: &[RemoteSessionRow], batch_id: u64) -> Vec<RemoteMessage> {
    let whole = RemoteMessage::Sessions {
        rows: rows.to_vec(),
    };
    if encode_remote_message(&whole).len() <= medulla_link::MAX_MESSAGE_BYTES {
        return vec![whole];
    }

    let mut groups: Vec<Vec<RemoteSessionRow>> = Vec::new();
    let mut group = Vec::new();
    for source in rows {
        let mut row = source.clone();
        let candidate_fits = |candidate: &[RemoteSessionRow]| {
            encode_remote_message(&RemoteMessage::SessionsChunk {
                batch_id,
                index: u16::MAX,
                count: u16::MAX,
                rows: candidate.to_vec(),
            })
            .len()
                <= medulla_link::MAX_MESSAGE_BYTES
        };
        let mut candidate = group.clone();
        candidate.push(row.clone());
        if !candidate_fits(&candidate) && !group.is_empty() {
            groups.push(std::mem::take(&mut group));
            candidate = vec![row.clone()];
        }
        if !candidate_fits(&candidate) {
            shrink_session_row_to_fit(&mut row, &candidate_fits);
            candidate = vec![row.clone()];
        }
        assert!(
            candidate_fits(&candidate),
            "a session row's stable id exceeds the remote datagram budget"
        );
        group.push(row);
    }
    if !group.is_empty() || groups.is_empty() {
        groups.push(group);
    }
    let count = groups.len().min(u16::MAX as usize) as u16;
    let messages: Vec<_> = groups
        .into_iter()
        .enumerate()
        .map(|(index, rows)| RemoteMessage::SessionsChunk {
            batch_id,
            index: index.min(u16::MAX as usize) as u16,
            count,
            rows,
        })
        .collect();
    assert!(messages.iter().all(|message| {
        encode_remote_message(message).len() <= medulla_link::MAX_MESSAGE_BYTES
    }));
    messages
}

/// Trim only optional usage until the full reliable-channel message fits one
/// datagram. Session identity/state fields are never removed here.
fn shrink_session_row_to_fit(
    row: &mut RemoteSessionRow,
    fits: &impl Fn(&[RemoteSessionRow]) -> bool,
) {
    loop {
        if fits(std::slice::from_ref(row)) {
            return;
        }
        if let Some(usage) = row.usage.as_mut() {
            if !usage.cpu_history.is_empty() {
                usage.cpu_history.remove(0);
                continue;
            }
            row.usage = None;
            continue;
        }
        let fields = [
            (row.label.len(), 0),
            (row.provider.len(), 1),
            (row.cwd.len(), 2),
            (row.preset.as_ref().map_or(0, String::len), 3),
            (row.name.as_ref().map_or(0, String::len), 4),
            (row.thread_name.as_ref().map_or(0, String::len), 5),
            (row.last_error.as_ref().map_or(0, String::len), 6),
            (
                row.attention
                    .as_ref()
                    .map_or(0, |attention| attention.summary.len()),
                7,
            ),
            (
                row.attention
                    .as_ref()
                    .map_or(0, |attention| attention.kind.len()),
                8,
            ),
        ];
        let Some((length, index)) = fields.into_iter().max_by_key(|(length, _)| *length) else {
            return;
        };
        if length == 0 {
            return;
        }
        let value = match index {
            0 => &mut row.label,
            1 => &mut row.provider,
            2 => &mut row.cwd,
            3 => row.preset.as_mut().expect("non-empty preset"),
            4 => row.name.as_mut().expect("non-empty name"),
            5 => row.thread_name.as_mut().expect("non-empty thread name"),
            6 => row.last_error.as_mut().expect("non-empty error"),
            7 => &mut row.attention.as_mut().expect("non-empty attention").summary,
            _ => &mut row.attention.as_mut().expect("non-empty attention").kind,
        };
        truncate_to_char_boundary(value, value.len() / 2);
    }
}

pub(super) fn truncate_to_char_boundary(value: &mut String, mut target: usize) {
    while !value.is_char_boundary(target) {
        target -= 1;
    }
    value.truncate(target);
}
