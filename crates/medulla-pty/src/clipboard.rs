//! Clipboard output callback used by a PTY session manager.

use std::sync::Arc;

/// Receives text copied by a harness through OSC 52.
pub type ClipboardSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Decode a valid OSC 52 clipboard write, accepting padded or unpadded base64.
pub fn osc52_from_params(params: &[&[u8]]) -> Option<String> {
    if *params.first()? != &b"52"[..] || params.len() < 2 {
        return None;
    }
    let payload = params.last()?;
    if payload.is_empty() || *payload == b"?" {
        return None;
    }
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
        .ok()?;
    String::from_utf8(bytes).ok()
}
