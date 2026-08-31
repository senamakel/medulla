//! Unit tests for the launch-time access gate's parts that need no terminal:
//! the startup note a grant is announced with, and the sign-out the gate screen
//! offers.

use std::collections::HashMap;
use std::sync::Mutex;

use medulla::access::{Grant, Plan};

use super::{sign_out, startup_note};

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

#[test]
fn a_paid_account_is_not_told_anything() {
    assert_eq!(startup_note(Grant::Paid(Plan::Basic)), None);
    assert_eq!(startup_note(Grant::Paid(Plan::Pro)), None);
}

#[test]
fn an_admin_grant_is_not_announced() {
    assert_eq!(startup_note(Grant::AdminGranted), None);
}

#[test]
fn a_trial_counts_the_days_down() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 5 * DAY_MS,
    });
    assert_eq!(
        note.as_deref(),
        Some(
            "Free trial: 5 days left. Medulla needs a Basic subscription after that — \
             https://tinyhumans.ai/pricing"
        )
    );
}

/// Rounding up, so a partial day is never reported as none left.
#[test]
fn a_partial_day_rounds_up_rather_than_reading_as_expired() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 11 * 60 * 60 * 1_000,
    });
    assert!(note.unwrap().starts_with("Free trial: 1 day left."));
}

#[test]
fn the_last_day_is_phrased_in_the_singular() {
    let exactly_one = startup_note(Grant::Trial {
        remaining_ms: DAY_MS,
    });
    assert!(exactly_one.unwrap().starts_with("Free trial: 1 day left"));

    let two = startup_note(Grant::Trial {
        remaining_ms: 2 * DAY_MS,
    });
    assert!(two.unwrap().starts_with("Free trial: 2 days left"));
}

#[test]
fn a_full_window_reads_as_thirty_days() {
    let note = startup_note(Grant::Trial {
        remaining_ms: medulla::access::TRIAL.as_millis() as i64,
    });
    assert!(note.unwrap().starts_with("Free trial: 30 days left"));
}

/// The countdown is the only place a trialling operator is told what happens
/// when it runs out, so it has to say.
#[test]
fn the_countdown_names_the_subscription_it_ends_in() {
    let note = startup_note(Grant::Trial {
        remaining_ms: 3 * DAY_MS,
    })
    .unwrap();
    assert!(note.contains("Basic subscription"), "{note}");
    assert!(note.contains("https://tinyhumans.ai/pricing"), "{note}");
}

// --- signing out from the gate -------------------------------------------

/// An env pointing every Medulla path at a scratch root, so a test never reads
/// or removes the developer's own session.
fn scratch_env(root: &std::path::Path) -> HashMap<String, String> {
    HashMap::from([("MEDULLA_HOME".to_string(), root.display().to_string())])
}

/// Write a session file where `medulla::auth::clear` will look for it.
fn seed_session(env: &HashMap<String, String>) -> std::path::PathBuf {
    let home = medulla::home::medulla_home(env);
    std::fs::create_dir_all(&home).unwrap();
    let path = home.join("session.json");
    std::fs::write(&path, r#"{"token":"t","userId":"u1"}"#).unwrap();
    path
}

/// Serializes the tests below: `sign_out` sweeps legacy credentials through
/// `adopt_legacy_credentials`, whose OS-config-directory location comes from
/// `dirs::config_dir()` — the *process's* real environment, not the `env` map
/// `sign_out` otherwise takes every path from. Without redirecting
/// `XDG_CONFIG_HOME` too, a developer's own `~/.config/medulla/credentials.json`
/// would be read, and deleted, by an ordinary test run. Two tests racing on
/// that same mutation would each think it owned the value.
static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

/// Points the OS config directory at a scratch root for the life of one test,
/// alongside `MEDULLA_HOME`, so `sign_out`'s legacy-credential sweep only ever
/// sees files this test wrote. Restores whatever was there before on drop.
struct ScratchConfigDir {
    _guard: std::sync::MutexGuard<'static, ()>,
    _dir: tempfile::TempDir,
    previous: Option<String>,
}

impl ScratchConfigDir {
    fn install() -> Self {
        let guard = CONFIG_DIR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("a scratch config directory");
        let previous = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        Self {
            _guard: guard,
            _dir: dir,
            previous,
        }
    }
}

impl Drop for ScratchConfigDir {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}

#[test]
fn signing_out_removes_the_stored_session() {
    let _config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();
    let env = scratch_env(root.path());
    let path = seed_session(&env);

    let msg = sign_out(&env, &medulla::config::BackendConfig::default());

    assert!(!path.exists(), "the bearer is still on disk");
    assert_eq!(msg, "Signed out. Run `medulla` to sign in again.");
}

/// Idempotent, the same way `medulla logout` is: nothing stored is not a
/// failure to report.
#[test]
fn signing_out_with_nothing_stored_still_reports_a_clean_sign_out() {
    let _config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();

    let msg = sign_out(
        &scratch_env(root.path()),
        &medulla::config::BackendConfig::default(),
    );

    assert_eq!(msg, "Signed out. Run `medulla` to sign in again.");
}

/// A token from the environment outranks the store, so clearing the store
/// leaves the next launch signed in as the same account — and landing back on
/// this screen would look like the sign-out silently failed.
#[test]
fn an_environment_token_is_reported_because_it_survives_the_sign_out() {
    let _config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();
    let mut env = scratch_env(root.path());
    let backend = medulla::config::BackendConfig::default();
    env.insert(backend.token_env.clone(), "still-here".to_string());
    let path = seed_session(&env);

    let msg = sign_out(&env, &backend);

    // The removal still happens: what remains is a source this cannot reach.
    assert!(!path.exists(), "the stored session was left behind");
    assert!(msg.contains(&backend.token_env), "{msg}");
    assert!(msg.contains("still authenticated"), "{msg}");
}

#[test]
fn an_inline_config_token_is_named_as_the_config_rather_than_the_environment() {
    let _config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();
    let backend = medulla::config::BackendConfig {
        token: Some("inline".to_string()),
        ..Default::default()
    };

    let msg = sign_out(&scratch_env(root.path()), &backend);

    assert!(msg.contains("`backend.token` in the config"), "{msg}");
}

/// A legacy `credentials.json` still in the OS config directory (the older of
/// the two locations `adopt_legacy_credentials` sweeps) is removed by signing
/// out, exactly as `medulla logout` removes it — so a re-launch cannot adopt
/// it and sign the operator straight back in.
#[test]
fn signing_out_removes_a_legacy_credential_from_the_config_directory() {
    let config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();

    let legacy_dir = config._dir.path().join("medulla");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("credentials.json");
    std::fs::write(
        &legacy_path,
        r#"{"baseUrl":"https://example.test","jwt":"legacy-token","userId":"u1"}"#,
    )
    .unwrap();

    let msg = sign_out(
        &scratch_env(root.path()),
        &medulla::config::BackendConfig::default(),
    );

    assert!(
        !legacy_path.exists(),
        "the legacy credential is still on disk"
    );
    assert_eq!(msg, "Signed out. Run `medulla` to sign in again.");
}

/// A legacy credential that cannot be removed — a read-only mount, a
/// permissions problem — must not be reported as a clean sign-out: that file
/// is exactly the bearer a re-launch would adopt, signing the operator back in
/// to the account they just left. Simulated here by making its directory
/// non-writable, which is Unix-only.
#[cfg(unix)]
#[test]
fn a_legacy_credential_that_cannot_be_removed_is_named_in_the_parting_message() {
    use std::os::unix::fs::PermissionsExt;

    let config = ScratchConfigDir::install();
    let root = tempfile::tempdir().unwrap();

    let legacy_dir = config._dir.path().join("medulla");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("credentials.json");
    std::fs::write(
        &legacy_path,
        r#"{"baseUrl":"https://example.test","jwt":"legacy-token","userId":"u1"}"#,
    )
    .unwrap();
    // Deny write on the directory: the file itself stays readable (so it is
    // still adopted as a candidate), but `remove_file` cannot unlink it.
    std::fs::set_permissions(&legacy_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let msg = sign_out(
        &scratch_env(root.path()),
        &medulla::config::BackendConfig::default(),
    );

    // Restore write access so the temp directory can be cleaned up.
    std::fs::set_permissions(&legacy_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(legacy_path.exists(), "the legacy credential should survive");
    assert!(
        msg.contains("could not be removed"),
        "the parting message must say the removal failed: {msg}"
    );
    assert!(msg.contains("credentials.json"), "{msg}");
}
