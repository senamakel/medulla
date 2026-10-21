use crate::{CompressInput, Compressor, Passthrough};

#[test]
fn passthrough_preserves_input_and_reports_identical_estimates() {
    let input = CompressInput::new("Keep this context byte-for-byte, 日本語 too.");

    let compressed = Passthrough.compress(&input);

    assert_eq!(compressed.text.as_bytes(), input.text.as_bytes());
    assert_eq!(compressed.stats.before_bytes, compressed.stats.after_bytes);
    assert_eq!(compressed.stats.before_tokens, compressed.stats.after_tokens);
}
