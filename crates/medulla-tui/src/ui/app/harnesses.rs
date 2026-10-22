//! Collect facts for the Harnesses Settings page away from the render path.

pub(crate) fn collect_harnesses(
    _config: &medulla::config::TuiConfig,
    _sources: &[String],
    _medulla_home: &std::path::Path,
    _env: &std::collections::HashMap<String, String>,
) -> super::types::HarnessesView {
    super::types::HarnessesView::default()
}

#[cfg(all(test, unix))]
mod tests {
    use super::collect_harnesses;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn collector_uses_injected_path_and_effective_customizations() {
        let temp = tempfile::tempdir().unwrap();
        let bin_dir = temp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let claude = bin_dir.join("claude");
        std::fs::write(&claude, "#!/bin/sh\nprintf 'Claude Code 2.4.1\\n'\n").unwrap();
        let mut permissions = std::fs::metadata(&claude).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&claude, permissions).unwrap();

        let source = temp.path().join("medulla.tui.json");
        std::fs::write(
            &source,
            r#"{"customHarnesses":[{"id":"codex-router","name":"Codex Router","baseHarness":"codex","model":"openai/gpt-4.1","hostId":"local","codexOverrides":true}]}"#,
        )
        .unwrap();
        let home = temp.path().join("medulla-home");
        std::fs::create_dir_all(&home).unwrap();
        let env = HashMap::from([
            ("PATH".to_string(), bin_dir.display().to_string()),
            ("HOME".to_string(), temp.path().display().to_string()),
        ]);

        let view = collect_harnesses(
            &medulla::config::TuiConfig::default(),
            &[source.display().to_string()],
            &home,
            &env,
        );

        assert!(view.detection_complete);
        assert_eq!(view.providers.len(), 1);
        assert_eq!(view.providers[0].name, "claude");
        assert_eq!(view.providers[0].version.as_deref(), Some("Claude Code 2.4.1"));
        assert!(view
            .customizations
            .iter()
            .any(|fact| fact.contains("Codex overrides") && fact.contains('1')));
        assert!(view
            .customizations
            .iter()
            .any(|fact| fact.contains("skills") && fact.contains('0')));
    }
}
