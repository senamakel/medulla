//! Owner-only files and per-socket MCP state paths.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The real environment of *this* process — the daemon or TUI itself, not any
/// task's spawn environment — read only to resolve where this account's own
/// runtime state lives ([`crate::home::medulla_home`]).
///
/// Deliberately not the `env` a caller such as [`attach_cli`] was handed: that
/// map describes a *task's* spawn, which a caller may scrub or override pieces
/// of for the child, and [`revoke_session`] — invoked from wherever a harness's
/// session is finally reaped, often nowhere near any per-task environment —
/// has no such map to consult even in principle. Both the write and the later
/// revoke have to agree on the same path for the same session, so both read
/// the one environment guaranteed to be present, and unchanged, in either
/// place: this process's own.
fn process_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

/// The subdirectory this process's own control plane's files are namespaced
/// under, keyed off the socket it bound.
///
/// Two Medulla processes can legitimately share one account home while bound
/// to two different sockets — `control_plane::start` in the `medulla-tui`
/// crate already documents that as a real, supported case ("a machine running
/// two Medullas"). Keying the subdirectory to *this* process's own socket
/// means [`sweep_stale_config_files`] only ever reads and removes files under
/// that same key: a sibling process bound to a different socket writes into a
/// different subdirectory entirely, so restarting one instance can never
/// delete a file the other still needs. Hashed rather than used verbatim so
/// an arbitrary configured socket path — which can contain anything a
/// filesystem allows — always yields one safe path component, the same
/// technique `workflows::store::file::dirs` already uses to scope a write
/// destination.
///
/// `"no-plane"` when this process bound none: reachable only if a caller
/// outside this crate reaches [`ServerSpec::write_config_file`] directly
/// without going through [`local_fleet_grant`], since every real caller here
/// requires an active plane before there is anything secret to write at all —
/// and with no plane bound, there is no sibling instance's files to protect
/// from, either.
fn instance_key() -> String {
    match crate::control_socket::active() {
        Some(plane) => socket_key(&plane.socket),
        None => "no-plane".to_string(),
    }
}

/// [`instance_key`] for a socket named directly, for the one caller that has
/// bound a plane but not yet published it — see [`sweep_stale_config_files`].
fn socket_key(socket: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(socket.as_os_str().as_encoded_bytes())
    )
}

/// Where this account's MCP config files live:
/// `<medulla_home>/mcp/<instance_key>`.
pub(super) fn mcp_state_dir() -> PathBuf {
    state_dir_for(&instance_key())
}

/// Where the `--mcp-config` files of the instance bound to `socket` live.
///
/// Public because the directory is part of this module's observable contract,
/// not an implementation detail: [`sweep_stale_config_files`] empties it, and a
/// caller that wants to assert on — or clean up after — what a bind did needs
/// to be able to name it without re-deriving the hash.
pub fn config_dir_for_socket(socket: &Path) -> PathBuf {
    state_dir_for(&socket_key(socket))
}

/// [`mcp_state_dir`] for an already-computed key.
fn state_dir_for(key: &str) -> PathBuf {
    crate::home::medulla_home(&process_env())
        .join("mcp")
        .join(key)
}

/// Whether `session` is safe to use as this file's sole filename component.
///
/// Every session key this crate mints itself is a UUID-derived string (see
/// `worker::pty::launch::attach_mcp` in the `medulla-tui` crate), but
/// [`attach_cli`], [`ServerSpec::write_config_file`], and [`revoke_session`]
/// are public API a caller outside this crate could hand an arbitrary string.
/// `PathBuf::join` treats an argument containing `/` as more than one path
/// component and an *absolute* argument as a full replacement of the base
/// path it is joined onto — so an unchecked session could write the secret
/// outside [`mcp_state_dir`] entirely, or make [`revoke_session`] delete a
/// file that has nothing to do with it.
///
/// Asked of the platform rather than spelled out as a list of forbidden
/// characters, because that list was wrong: a denylist of `/`, `\\` and NUL
/// accepts `C:temp`, which on Windows is a *drive-relative* path, so
/// `join`ing it replaces the state directory exactly the way an absolute path
/// would. `Path::components` is the only thing that knows every such syntax
/// on the platform actually running, so the test is structural — the string
/// must decompose into exactly one component, and that component must be the
/// whole string back again. `.` and `..` fail it for free, being `CurDir` and
/// `ParentDir` rather than `Normal`.
///
/// A colon and a backslash are rejected outright on top of that, and for one
/// reason: both are ordinary filename characters on Unix, so `components`
/// there answers "one component" for `C:temp` and `back\\slash` — correctly,
/// on Unix — while on Windows the first is drive-relative and the second is a
/// separator. Accepting a key on one platform that escapes on another is a
/// trap for whoever reads this next, so the structural test is kept for what
/// only the platform can answer and these two are refused everywhere. Every
/// key this crate mints is UUID-derived and contains neither.
fn is_safe_session_component(session: &str) -> bool {
    if session.is_empty()
        || session.contains('\0')
        || session.contains(':')
        || session.contains('\\')
    {
        return false;
    }
    let mut components = Path::new(session).components();
    let Some(std::path::Component::Normal(only)) = components.next() else {
        return false;
    };
    only == std::ffi::OsStr::new(session) && components.next().is_none()
}

/// Where a session's [`ServerSpec::write_config_file`] output would be, if it
/// wrote one — `None` when `session` fails [`is_safe_session_component`],
/// which both the write and the revoke below refuse to touch a filesystem
/// path for.
///
/// Deterministic from the session key alone: [`revoke_session`] needs to find
/// and remove the same file without any caller having to hand the path back.
pub(super) fn config_file_path(session: &str) -> Option<PathBuf> {
    is_safe_session_component(session).then(|| mcp_state_dir().join(format!("{session}.json")))
}

/// Remove every config file a *previous* run of this account's control plane
/// left behind.
///
/// [`revoke_session`] is the normal way a file here stops existing, but it
/// only runs when a session is reaped in the same process that minted its
/// grant — a process that exits without reaching that point (killed, or
/// crashed) leaves its file on disk with nothing left to remove it. This is
/// Takes the bound `socket` rather than reading
/// [`crate::control_socket::active`], because the one correct moment to call
/// it is precisely when that is still `None`: the directory these files live
/// in is keyed off the socket (see `instance_key`), so a sweep that resolved
/// the key from an unpublished plane would clear `no-plane` — some other
/// caller's directory — and leave this instance's own leftovers untouched.
///
/// This is the mitigation for that gap, called once at startup — after the
/// control plane binds but strictly *before* it is published to
/// [`crate::control_socket::active`] (see `control_plane::start` in the
/// `medulla-tui` crate). The ordering matters: once a plane is visible, a task
/// arriving on another thread can launch a session and write its config file
/// immediately, and a sweep running after that point would delete a file
/// belonging to a live session. Sweeping first means every file this run
/// writes comes after the last deletion this run performs. Every grant
/// a prior process minted lived only in that process's in-memory
/// [`crate::control_socket::GrantRegistry`], which died with the process, so a
/// leftover file's token can never be redeemed again no matter how long it
/// sits on disk — sweeping it is hygiene, not a race against something still
/// live. Best effort throughout: a directory that does not exist yet, or one
/// entry this account cannot remove, is not worth failing startup over.
pub fn sweep_stale_config_files(socket: &Path) {
    let Ok(entries) = std::fs::read_dir(state_dir_for(&socket_key(socket))) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Write `contents` to `path`, created — not opened, not truncated — with
/// permissions only its owner can read.
///
/// `create_new` (`O_CREAT | O_EXCL`) is what actually closes the class of
/// attack the module docs describe, not the mode alone: `session` leans on a
/// fresh UUID for its randomness, but a pre-planted entry at the exact target
/// path — including a symlink pointed somewhere the caller does not expect —
/// would still be followed and written through by a plain `create`. Failing
/// the open instead means the secret is never written to the wrong place; the
/// caller's fallback is the non-secret inline document, which needs no file at
/// all. Windows has no exclusive-creation flag through `std::fs`, but
/// `OpenOptions::create_new` maps to `CREATE_NEW` there and gets the same
/// property; only the owner-only *mode* is Unix-specific.
#[cfg(unix)]
pub(super) fn write_owner_only(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

/// Windows fallback: no owner-only mode bit, but the same exclusive-creation
/// guarantee against a pre-planted path.
#[cfg(not(unix))]
pub(super) fn write_owner_only(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes())
}
