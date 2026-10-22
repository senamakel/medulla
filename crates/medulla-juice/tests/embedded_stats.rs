use medulla_juice::{read_embedded_stats, EmbeddedCompressionStats};
use std::io::ErrorKind;

fn write_stats(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    let state = workspace.join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("tokenjuice_savings.json"), body).unwrap();
    (dir, workspace)
}

#[test]
fn reads_only_embedded_token_totals_and_ignores_future_fields() {
    let (_dir, workspace) = write_stats(
        r#"{"total":{"events":2,"originalTokens":1200,"compactedTokens":300,"tokensSaved":900,"costSavedUsd":0.01},"byModel":{},"future":true}"#,
    );
    assert_eq!(
        read_embedded_stats(&workspace).unwrap(),
        Some(EmbeddedCompressionStats {
            events: 2,
            tokens: medulla_juice::CompressionTokenTotals {
                before_tokens: 1200,
                after_tokens: 300
            },
        })
    );
}

#[test]
fn missing_file_is_none_but_malformed_or_inverted_data_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        read_embedded_stats(&dir.path().join("absent")).unwrap(),
        None
    );
    for body in [
        "{",
        r#"{"total":{"events":1,"originalTokens":3,"compactedTokens":4}}"#,
    ] {
        let (_dir, workspace) = write_stats(body);
        assert_eq!(
            read_embedded_stats(&workspace).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
}

#[test]
fn oversized_file_is_rejected() {
    let (_dir, workspace) =
        write_stats(&format!("{{\"padding\":\"{}\"}}", "x".repeat(1024 * 1024)));
    assert_eq!(
        read_embedded_stats(&workspace).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn saved_tokens_saturate() {
    let stats = medulla_juice::CompressionTokenTotals {
        before_tokens: 3,
        after_tokens: 4,
    };
    assert_eq!(stats.saved_tokens(), 0);
}
