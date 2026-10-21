//! Clipboard output callback used by a PTY session manager.

use std::sync::Arc;

/// Receives text copied by a harness through OSC 52.
pub type ClipboardSink = Arc<dyn Fn(&str) + Send + Sync>;
