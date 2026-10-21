pub struct CompressInput<'a> {
    pub text: &'a str,
}

impl<'a> CompressInput<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }
}

pub struct CompressionStats {
    pub before_bytes: usize,
    pub after_bytes: usize,
    pub before_tokens: u64,
    pub after_tokens: u64,
}

pub struct Compressed {
    pub text: String,
    pub stats: CompressionStats,
}

pub trait Compressor {
    fn compress(&self, input: &CompressInput<'_>) -> Compressed;
}

pub struct Passthrough;

impl Compressor for Passthrough {
    fn compress(&self, input: &CompressInput<'_>) -> Compressed {
        Compressed {
            text: input.text.to_owned(),
            stats: CompressionStats {
                before_bytes: input.text.len(),
                after_bytes: 0,
                before_tokens: 0,
                after_tokens: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests;
