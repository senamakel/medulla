//! Unit tests for the launch-time access gate's pure parts.

use medulla::access::{Grant, Plan};

use super::startup_note;

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

use std::collections::HashMap;

use super::sign_out;

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

#[test]
fn signing_out_removes_the_stored_session() {
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
    let root = tempfile::tempdir().unwrap();
    let backend = medulla::config::BackendConfig {
        token: Some("inline".to_string()),
        ..Default::default()
    };

    let msg = sign_out(&scratch_env(root.path()), &backend);

    assert!(msg.contains("`backend.token` in the config"), "{msg}");
}
