//! Pure coverage for [`crate::remote::serve::paired::reexec_args`]: the
//! `daemon` subcommand must survive into the re-exec's argv (`main`'s own
//! dispatch already stripped it from what `run` receives), the host key must
//! never survive, and `--host` must always be present so the re-invocation
//! reads pairings from disk.

use crate::cli::{parse_command, Command};
use crate::remote::serve::paired::reexec_args;

#[test]
fn restores_daemon_drops_the_key_and_adds_host() {
    // `args` here is what `paired::run` actually receives: `main` hands it
    // `&raw[1..]`, which already has `daemon` stripped off — the same shape
    // `flag()` elsewhere in this module expects.
    let args = vec![
        "HK1-SECRET-KEY".to_string(),
        "--workspace".to_string(),
        "/work".to_string(),
    ];
    let out = reexec_args(&args, "HK1-SECRET-KEY");
    assert!(
        !out.iter().any(|arg| arg == "HK1-SECRET-KEY"),
        "the key must not appear in the re-exec argv: {out:?}"
    );
    assert!(
        out.iter().any(|arg| arg == "--host"),
        "the re-exec must ask for --host mode: {out:?}"
    );
    assert_eq!(
        out,
        vec![
            "daemon".to_string(),
            "--workspace".to_string(),
            "/work".to_string(),
            "--host".to_string(),
        ],
        "the re-exec argv must still start with the daemon subcommand, or \
         parse_command reads it as the interactive TUI"
    );
}

#[test]
fn does_not_duplicate_an_existing_host_flag() {
    // Defensive: `run` only calls this on the pasted-key path, where
    // `--host` is never already present, but the helper should still be
    // idempotent if that ever changes.
    let args = vec!["HK1-SECRET-KEY".to_string(), "--host".to_string()];
    let out = reexec_args(&args, "HK1-SECRET-KEY");
    assert_eq!(out.iter().filter(|arg| *arg == "--host").count(), 1);
}

#[test]
fn preserves_argument_order_and_other_flags() {
    let args = vec![
        "--host-name".to_string(),
        "box".to_string(),
        "HK1-SECRET-KEY".to_string(),
        "--workspaces".to_string(),
        "/a,/b".to_string(),
    ];
    let out = reexec_args(&args, "HK1-SECRET-KEY");
    assert_eq!(
        out,
        vec![
            "daemon".to_string(),
            "--host-name".to_string(),
            "box".to_string(),
            "--workspaces".to_string(),
            "/a,/b".to_string(),
            "--host".to_string(),
        ]
    );
}
