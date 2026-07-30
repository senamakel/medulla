//! Focused tests for bounded folder completion and fuzzy ranking.

use super::harness_workspace::{folder_completions, fuzzy_subsequence_score};

#[test]
fn fuzzy_matching_accepts_tight_subsequences_and_rejects_missing_characters() {
    assert!(fuzzy_subsequence_score("workflow-medulla", "wfm").is_some());
    assert!(
        fuzzy_subsequence_score("workflow-medulla", "workflow")
            < fuzzy_subsequence_score("workflow-medulla", "wfm")
    );
    assert_eq!(fuzzy_subsequence_score("workflow-medulla", "xyz"), None);
}

#[test]
fn folder_completion_lists_matching_children_but_never_files() {
    let root = tempfile::tempdir().unwrap();
    let alpha = root.path().join("project-alpha");
    let beta = root.path().join("project-beta");
    std::fs::create_dir(&alpha).unwrap();
    std::fs::create_dir(&beta).unwrap();
    std::fs::write(root.path().join("project-not-a-folder"), "no").unwrap();

    let query = root.path().join("pb").to_string_lossy().into_owned();
    let results = folder_completions(&query);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, beta.to_string_lossy());
}
