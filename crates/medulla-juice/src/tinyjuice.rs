use crate::{CompressInput, Compressed, CompressionStats, Compressor};
use tinyjuice::compressors::json::SmartCrusherTableTransform;
use tinyjuice::{ContentKind, PipelineInput, ReformatTransform};

/// Faithful JSON table reformatting backed by the workspace's pinned TinyJuice.
#[derive(Clone, Copy, Debug, Default)]
pub struct TinyJuice;

impl Compressor for TinyJuice {
    fn compress(&self, input: &CompressInput<'_>) -> Compressed {
        let before_tokens = estimate_tokens(input.text);
        let pipeline_input = PipelineInput {
            content: input.text,
            original_content: input.text,
            content_kind: ContentKind::Json,
            original_bytes: input.text.len(),
        };
        let candidate = SmartCrusherTableTransform
            .apply(&pipeline_input)
            .map(|output| output.text)
            .filter(|text| {
                let after_bytes = text.len();
                let after_tokens = estimate_tokens(text);
                after_bytes <= input.text.len()
                    && after_tokens <= before_tokens
                    && (after_bytes < input.text.len() || after_tokens < before_tokens)
            });
        let text = candidate.unwrap_or_else(|| input.text.to_owned());
        Compressed {
            stats: CompressionStats {
                before_bytes: input.text.len(),
                after_bytes: text.len(),
                before_tokens,
                after_tokens: estimate_tokens(&text),
            },
            text,
        }
    }
}

fn estimate_tokens(text: &str) -> u64 {
    text.chars().count().div_ceil(4) as u64
}
