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
