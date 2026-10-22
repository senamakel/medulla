use super::types::{App, Cmd};
use super::types::SP_TOKENS;

const REFRESH_SECONDS: u64 = 5;

impl App {
    pub fn due_compression_refresh(&mut self, now_seconds: u64) -> Option<Cmd> {
        if !cfg!(feature = "tinyjuice")
            || self.tab() != "Settings"
            || self.settings_index != SP_TOKENS
            || self.compression_loading
            || now_seconds.saturating_sub(self.compression_refreshed_at) < REFRESH_SECONDS
        {
            return None;
        }
        self.compression_loading = true;
        self.compression_refreshed_at = now_seconds;
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        Some(Cmd::LoadCompressionStats {
            workspace: medulla::home::medulla_home(&env).join("embed").join("workspace"),
        })
    }

    pub fn set_compression_stats(
        &mut self,
        result: Result<Option<medulla::juice::EmbeddedCompressionStats>, String>,
    ) {
        self.compression_loading = false;
        match result {
            Ok(stats) => self.compression = stats,
            Err(error) => self.set_status(format!("embedded compression stats unavailable: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let runtime: std::sync::Arc<dyn medulla::runtime::Runtime> =
            std::sync::Arc::new(medulla::runtime::mock::MockRuntime::demo());
        App::new(
            runtime,
            medulla::config::LoadedConfig::defaults("medulla.tui.json".into()),
        )
    }

    fn stats() -> medulla::juice::EmbeddedCompressionStats {
        medulla::juice::EmbeddedCompressionStats {
            events: 2,
            tokens: medulla::juice::CompressionTokenTotals {
                before_tokens: 1200,
                after_tokens: 300,
            },
        }
    }

    #[cfg(feature = "tinyjuice")]
    #[test]
    fn visible_tokens_page_refreshes_at_most_every_five_seconds() {
        let mut app = app();
        app.tab_index = super::super::types::tab_pos("Settings");
        app.settings_index = super::super::types::SP_TOKENS;
        assert!(matches!(app.due_compression_refresh(10), Some(Cmd::LoadCompressionStats { .. })));
        assert!(app.compression_loading);
        assert!(app.due_compression_refresh(20).is_none());
        app.compression_loading = false;
        assert!(app.due_compression_refresh(14).is_none());
        assert!(matches!(app.due_compression_refresh(15), Some(Cmd::LoadCompressionStats { .. })));
        app.tab_index = super::super::types::tab_pos("Sessions");
        app.compression_loading = false;
        assert!(app.due_compression_refresh(30).is_none());
    }

    #[cfg(not(feature = "tinyjuice"))]
    #[test]
    fn disabled_feature_never_schedules_workspace_reads() {
        let mut app = app();
        app.tab_index = super::super::types::tab_pos("Settings");
        app.settings_index = super::super::types::SP_TOKENS;
        assert!(app.due_compression_refresh(100).is_none());
    }

    #[test]
    fn missing_result_clears_data_but_transient_error_retains_last_good_value() {
        let mut app = app();
        app.compression = Some(stats());
        app.compression_loading = true;
        app.set_compression_stats(Err("truncated write".into()));
        assert_eq!(app.compression, Some(stats()));
        assert!(!app.compression_loading);
        app.set_compression_stats(Ok(None));
        assert_eq!(app.compression, None);
        app.set_compression_stats(Ok(Some(stats())));
        assert_eq!(app.compression, Some(stats()));
    }
}
