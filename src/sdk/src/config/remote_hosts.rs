//! Which remote machines are on offer, resolved from config alone.
//!
//! The sibling of [`local_hosts`](super::local_hosts), and it exists for the same
//! reason: the picker that *lists* remote hosts and the connector that *dials*
//! them must derive the same id and the same name from the same section. Two
//! derivations would eventually disagree, and a session filed under an id nobody
//! else uses is a session that cannot be found again.
//!
//! Resolution is config-only. It holds before anything has connected, which is
//! precisely when the picker needs it — the whole point of a lazily-opened
//! connection is that the host is listed long before it is dialled.

use super::RemoteHostSection;

/// One remote machine, as the picker and the connector both see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHostRef {
    /// Stable identifier, used in session ids and as the rail lane.
    pub id: String,
    /// What to call it on screen.
    pub name: String,
    /// The SSH destination — never empty, since an entry without one is dropped.
    pub host: String,
    /// The directory sessions there start in, as configured. Blank means the
    /// remote daemon's own default, which only that daemon can resolve.
    pub workspace: String,
}

/// Every remote host worth offering, in declaration order.
///
/// Two kinds of entry are dropped rather than shown, because in both cases a row
/// the operator cannot start a session on is worse than no row:
///
/// - one with no [`host`](RemoteHostSection::host), which cannot be dialled;
/// - one whose resolved id another entry already claimed. Ids name sessions, so
///   two hosts sharing one would make a remote session ambiguous. As with local
///   hosts, the first claim keeps it.
///
/// A [`disabled`](RemoteHostSection::enabled) entry is also skipped, but that is
/// the operator's own instruction rather than a collision.
pub fn remote_hosts(sections: &[RemoteHostSection]) -> Vec<RemoteHostRef> {
    let mut taken: Vec<String> = Vec::new();
    sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.enabled && !section.host.trim().is_empty())
        .map(|(index, section)| RemoteHostRef {
            id: remote_host_id(section, index),
            name: remote_host_name(section),
            host: section.host.trim().to_string(),
            workspace: section.workspace.trim().to_string(),
        })
        .filter(|host| {
            let fresh = !taken.contains(&host.id);
            if fresh {
                taken.push(host.id.clone());
            }
            fresh
        })
        .collect()
}

/// The id for one entry, derived from what it declared.
///
/// Preference order is deliberate: an explicit `id` is a promise the operator
/// made, a slugged `name` is stable across edits to everything else, and a
/// slugged `host` is the last thing that is still about *this* machine. Only when
/// all three are unusable does it fall back to the position in the file — which
/// is honest but fragile, since reordering the file would rename the host and
/// orphan its sessions.
pub fn remote_host_id(section: &RemoteHostSection, fallback_index: usize) -> String {
    for candidate in [&section.id, &section.name, &section.host] {
        let slug = slug_of(candidate);
        if !slug.is_empty() {
            return slug;
        }
    }
    format!("remote-host-{}", fallback_index + 1)
}

/// What to call a remote host on screen.
///
/// Falls back to the SSH destination, which is the one thing every usable entry
/// has. A machine known only as `tower.local` reads perfectly well; a machine
/// with a blank label does not.
pub fn remote_host_name(section: &RemoteHostSection) -> String {
    match section.name.trim() {
        "" => section.host.trim().to_string(),
        value => value.to_string(),
    }
}

/// A lowercase, hyphenated form of `name`, safe to use as an id.
///
/// Deliberately the same rule as [`local_hosts`](super::local_hosts)'s, so
/// `"GPU Box"` and `"gpu box"` collide here exactly as they would there.
fn slug_of(name: &str) -> String {
    let mut out = String::new();
    let mut hyphen = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            hyphen = false;
        } else if !out.is_empty() && !hyphen {
            out.push('-');
            hyphen = true;
        }
    }
    out.trim_end_matches('-').to_string()
}
